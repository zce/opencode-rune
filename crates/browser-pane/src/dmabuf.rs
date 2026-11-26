//! Linux: the engine's dmabuf frames, into each tab's surface. The engine copies every GPU frame
//! into a few dmabufs per tab, sends their descriptors once (`EngineMsg::DmabufPool`), then names
//! the buffer each frame is in (`EngineMsg::DmabufFrame`). Rune imports them on the app's Vulkan
//! device (`rune::native_surface::linux`); the app tells the engine once its GPU has read a frame
//! (`HostMsg::FrameRead`), and the engine writes that buffer again only after. When the app can't
//! show them, it says so (`HostMsg::DmabufFailed`) and the engine reads its frames back to BGRA.
//!
//! Everything goes to the connection the frame came on, never to a later engine's.

use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};

use rune::native_surface::linux::{DRM_FORMAT_ARGB8888, Dmabuf, DmabufBuffer, DmabufPlane, import_target};

use crate::frame::TabSurface;
use crate::protocol::HostMsg;

/// Whether this app imports the engine's dmabufs: it does until an import fails.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Import {
    Wanted,
    Failed,
}

enum ImportEvent {
    Failed,
}

static IMPORT: Mutex<Import> = Mutex::new(Import::Wanted);

/// True when the import just failed, so the engine is told once.
fn step(state: &mut Import, event: ImportEvent) -> bool {
    match (*state, event) {
        (Import::Wanted, ImportEvent::Failed) => {
            *state = Import::Failed;
            true
        }
        (Import::Failed, ImportEvent::Failed) => false,
    }
}

/// The render node an engine started now copies its frames on, when this app imports dmabufs:
/// the GPU Rune draws the window on.
pub(crate) fn device() -> Option<String> {
    if *IMPORT.lock().unwrap_or_else(PoisonError::into_inner) == Import::Failed {
        return None;
    }
    let target = import_target()?;
    let node = target.render_node.unwrap_or_else(|| "/dev/dri/renderD128".into());
    if nvidia_gbm(&node) {
        // NVIDIA's proprietary GBM can't make the linear ARGB8888 buffers the engine copies its
        // frames into (`GBM couldn't make a linear buffer`), and the engine's readback fallback
        // returns blank frames there. Paint BGRA instead; `engine_command` runs the engine on X11
        // so those frames are device pixels too.
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| eprintln!("browser: NVIDIA's GBM can't make the engine's dmabufs; painting bgra"));
        return None;
    }
    Some(node)
}

/// Whether `render_node` is on NVIDIA's proprietary driver, whose GBM has no linear ARGB8888
/// rendering buffers. `nouveau` and Mesa's NVK share the vendor id but not that limitation, so
/// the driver symlink decides.
fn nvidia_gbm(render_node: &str) -> bool {
    let Some(name) = std::path::Path::new(render_node).file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    std::fs::read_link(format!("/sys/class/drm/{name}/device/driver"))
        .ok()
        .and_then(|driver| driver.file_name().map(|name| name.to_string_lossy().into_owned()))
        .is_some_and(|driver| driver == "nvidia")
}

/// Tells the engine on `tx` that its dmabufs can't be shown, once per app.
fn fail(tx: &Sender<HostMsg>, reason: String) {
    if step(&mut IMPORT.lock().unwrap_or_else(PoisonError::into_inner), ImportEvent::Failed) {
        eprintln!("browser: can't show the engine's dmabufs ({reason}); asking for bgra");
    }
    tx.send(HostMsg::DmabufFailed { reason }).ok();
}

/// How the engine's frames have been arriving, so the change is logged once.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Frames {
    None,
    Dmabuf,
    Bgra,
}

impl Frames {
    pub(crate) fn step(&mut self, now: Frames) {
        if *self != now {
            eprintln!("browser: page frames over {}", if now == Frames::Dmabuf { "dmabuf" } else { "bgra" });
            *self = now;
        }
    }
}

/// A buffer set as the engine describes it.
pub(crate) struct PoolInfo {
    pub tab_id: u64,
    pub pool: u64,
    pub buffers: u32,
    pub size: [u32; 2],
    pub stride: u32,
    pub offset: u32,
    pub modifier: u64,
}

/// One connection's buffer sets, by tab id, and its own sender.
pub(crate) struct Pools {
    tabs: HashMap<u64, (u64, Vec<Arc<DmabufBuffer>>)>,
    tx: Sender<HostMsg>,
}

impl Pools {
    pub(crate) fn new(tx: Sender<HostMsg>) -> Self {
        Self { tabs: HashMap::new(), tx }
    }

    pub(crate) fn add(&mut self, info: PoolInfo, fds: Vec<OwnedFd>) {
        self.tabs.remove(&info.tab_id);
        if fds.len() != info.buffers as usize {
            return fail(&self.tx, format!("pool {} came with {} of {} descriptors", info.pool, fds.len(), info.buffers));
        }
        let buffers = fds.into_iter().map(|fd| {
            let plane = DmabufPlane { fd, offset: info.offset, stride: info.stride, modifier: info.modifier };
            DmabufBuffer::new(plane, info.size, DRM_FORMAT_ARGB8888).map(Arc::new)
        });
        match buffers.collect::<Result<Vec<_>, _>>() {
            Ok(buffers) => {
                self.tabs.insert(info.tab_id, (info.pool, buffers));
            }
            Err(reason) => fail(&self.tx, reason),
        }
    }

    /// Shows frame `frame_id`, in buffer `slot` of `pool`, on `surface`.
    pub(crate) fn show(&self, surface: Option<&TabSurface>, tab_id: u64, frame_id: u64, pool: u64, slot: u32) {
        let tx = self.tx.clone();
        // A tab the app already closed reads nothing: its buffer is free at once.
        let Some(surface) = surface else {
            tx.send(HostMsg::FrameRead { tab_id, pool, frame_id }).ok();
            return;
        };
        let Some(buffer) = self.tabs.get(&tab_id).filter(|(id, _)| *id == pool).and_then(|(_, b)| b.get(slot as usize)) else {
            return fail(&tx, format!("frame {frame_id} names buffer {slot} of pool {pool}, which the app doesn't have"));
        };
        let frame = Dmabuf::new(buffer.clone()).on_read(move |read| match read {
            Ok(()) => {
                tx.send(HostMsg::FrameRead { tab_id, pool, frame_id }).ok();
            }
            Err(reason) => fail(&tx, reason),
        });
        if let Err(reason) = surface.push_dmabuf(frame, frame_id) {
            fail(&self.tx, reason);
        }
    }

    /// The tab's buffers go: it closed, or its frames come as BGRA now.
    pub(crate) fn forget(&mut self, tab_id: u64) {
        self.tabs.remove(&tab_id);
    }
}

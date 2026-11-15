//! The rack's model and its rules, from record/areas/core/01-core.md: tiles in their order,
//! session states driven by Claude Code's hook events, timers on the wall clock, and saving your
//! own tiles. It holds no interface code, so the interface can be replaced without touching it.

pub mod cuts;
pub mod desktop;
pub mod hooks;
pub mod install;
pub mod rack;
pub mod receiver;
pub mod session;
pub mod store;
pub mod timer;
pub mod transcript;

pub use rack::{Body, Need, Own, Rack, Removed, Tile, TileId};
pub use session::{Session, SessionEvent, SessionState, Tool};
pub use timer::Timer;

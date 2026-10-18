//! OpenDownload Manager core: download engine, segment manager, queue,
//! scheduler and persistent storage. Independent of the UI framework.

pub mod category;
pub mod engine;
pub mod error;
pub mod filename;
pub mod http;
pub mod limiter;
pub mod manager;
pub mod model;
pub mod settings;
pub mod storage;
pub mod url_util;

pub use error::{DownloadError, ErrorCode, Result};
pub use manager::Manager;
pub use model::*;
pub use settings::Settings;
pub use storage::Store;

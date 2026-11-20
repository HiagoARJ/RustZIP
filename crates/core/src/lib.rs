//! Motor do RustZIP: lista, extrai, testa, cria e altera arquivos compactados.

pub mod create;
pub mod error;
pub mod extract;
pub mod format;
pub mod fsutil;
pub mod list;
pub mod progress;
pub mod smart;
pub mod source;
pub mod streams;
pub mod sys;
pub mod update;
pub mod volumes;

pub use create::{CreateOptions, CreateReport, ZipMethod, create};
pub use error::{Error, Result};
pub use extract::{ExtractOptions, Report, extract, test};
pub use format::Format;
pub use fsutil::Overwrite;
pub use list::{ArchiveInfo, Entry, list};
pub use progress::{Progress, Snapshot};
pub use update::{Edit, convert, modify};

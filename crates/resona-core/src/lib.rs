pub mod model;
pub mod parser;
pub mod query;
pub mod reducer;
pub mod storage;
pub use model::*;
pub use storage::Store;

mod legacy;

pub fn user_home() -> Result<std::path::PathBuf> {
    dirs::home_dir().ok_or_else(|| Error::Invalid("HOME_UNAVAILABLE".into()))
}

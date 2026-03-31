//! Database connection and operations

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;
use tokio::task;

pub struct Database {
    path: std::path::PathBuf,
}

impl Database {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let path_clone = path.clone();
        
        task::spawn_blocking(move || {
            let conn = Connection::open(&path_clone)?;
            crate::schema::init_schema(&conn)?;
            Ok(Database { path })
        })
        .await?
    }
    
    pub fn path(&self) -> &Path {
        &self.path
    }
}

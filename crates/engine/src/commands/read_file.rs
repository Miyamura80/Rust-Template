//! `read_file` – read a file and return its contents as a UTF-8 string.

use crate::commands::{Command, CommandError};
use crate::context::Ctx;
use crate::register_command;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Default)]
pub struct ReadFile;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadFileInput {
    /// Absolute path to the file to read.
    pub path: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ReadFileOutput {
    pub content: String,
    pub size_bytes: usize,
}

#[async_trait]
impl Command for ReadFile {
    type Input = ReadFileInput;
    type Output = ReadFileOutput;

    fn name(&self) -> &'static str {
        "read_file"
    }

    fn description(&self) -> &'static str {
        "Read a file and return its UTF-8 contents."
    }

    async fn run(
        &self,
        input: ReadFileInput,
        cx: &Ctx<'_>,
    ) -> Result<ReadFileOutput, CommandError> {
        let path = std::path::Path::new(&input.path);
        let data = cx.fs().read_file(path)?;
        let size_bytes = data.len();
        let content = String::from_utf8_lossy(&data).into_owned();
        Ok(ReadFileOutput {
            content,
            size_bytes,
        })
    }
}

register_command!(ReadFile);

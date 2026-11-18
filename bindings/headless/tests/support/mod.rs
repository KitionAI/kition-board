#![allow(dead_code)]

use kition_board_kit::BoardDocument;
use kition_kit::KitDocument;
use kition_kits::{
    KitDefinition, KitManager,
    tools::{self, CallOptions},
};
use serde_json::{Value, json};
use std::path::PathBuf;

pub struct Fixture {
    pub kits: KitManager,
    pub dir: tempfile::TempDir,
    pub a: Value,
    pub b: Value,
    pub frame: Value,
    pub child: Value,
    pub page: Value,
    pub other_page: Value,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = BoardDocument::create("Test.kboard", 1920, 1080).unwrap();
        let page = json!(doc.model().pages()[0].id.as_str());
        let made = doc
            .execute(
                "element.create",
                json!({"elements":[
                    {"kind":"shape","position":[0,0],"text":"A"},
                    {"kind":"shape","position":[300,0],"text":"B"},
                    {"kind":"shape","position":[30,430],"text":"Child"}
                ]}),
            )
            .unwrap();
        let (a, b, child) = (made["created"][0].clone(), made["created"][1].clone(), made["created"][2].clone());
        let frame = doc.execute("frame.create", json!({"box":[0,400,500,700],"children":[child],"title":"Frame"})).unwrap()["frame"].clone();
        let other_page = doc.execute("page.create", json!({"name":"Other"})).unwrap()["page"].clone();
        std::fs::write(dir.path().join("Test.kboard"), doc.save(None).unwrap()).unwrap();
        let definition = KitDefinition::from_manifest_text(include_str!("../../../../kit.json")).unwrap();
        let kits = KitManager::new(Some(dir.path().to_path_buf()), vec![(definition, Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-board"))))]);
        Self { kits, dir, a, b, child, frame, page, other_page }
    }

    pub fn file(&self) -> Vec<u8> {
        std::fs::read(self.dir.path().join("Test.kboard")).unwrap()
    }
    pub fn doc(&self) -> BoardDocument {
        BoardDocument::open("Test.kboard", &self.file()).unwrap()
    }

    /// A separate actor opens the current file and really replaces its bytes between calls.
    pub fn outside(&self, steps: &[(&str, Value)]) {
        let mut doc = self.doc();
        for (command, params) in steps {
            doc.execute(command, params.clone()).unwrap();
        }
        std::fs::write(self.dir.path().join("Test.kboard"), doc.save(None).unwrap()).unwrap();
    }

    pub async fn call(&self, action: &str, input: Value) -> Result<Value, Value> {
        self.call_with(action, input, CallOptions::default()).await
    }

    pub async fn call_with(&self, action: &str, mut input: Value, options: CallOptions) -> Result<Value, Value> {
        input["path"] = json!("Test.kboard");
        match tools::call_as(&self.kits, &format!("board_{action}"), &input, options, "agent-test").await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("no board_{action}"),
        }
    }
    pub async fn ok(&self, action: &str, input: Value) -> Value {
        self.call(action, input).await.unwrap_or_else(|error| panic!("board_{action}: {error}"))
    }
    pub async fn basis(&self) -> Value {
        self.ok("inspect", json!({})).await["revision"].clone()
    }
    pub async fn write(&self, basis: Value, command: &str, params: Value) -> Result<Value, Value> {
        self.call("run", json!({"basis":basis,"steps":[{"command":command,"params":params}]})).await
    }
    pub async fn shutdown(self) {
        self.kits.shutdown().await;
    }
}

pub fn conflict(answer: &Value, id: &Value, reason: &str) {
    assert_eq!(answer["error"]["code"], "conflict", "{answer}");
    assert!(answer["conflicts"].as_array().unwrap().iter().any(|item| item["id"] == *id && item["reason"] == reason), "{answer}");
}

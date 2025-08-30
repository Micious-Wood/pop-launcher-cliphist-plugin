use futures::prelude::*;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use pop_launcher::PluginResponse;
use pop_launcher::*;
use std::{borrow::Cow, cmp::Ordering, collections::HashMap, process::Command};
use tokio::io::{AsyncWrite, AsyncWriteExt};

pub async fn send<W: AsyncWrite + Unpin>(tx: &mut W, response: PluginResponse) {
    if let Ok(mut bytes) = serde_json::to_string(&response) {
        bytes.push('\n');
        let _ = tx.write_all(bytes.as_bytes()).await;
    }
}

/// Run both futures and take the output of the first one to finish.
pub async fn or<T>(future1: impl Future<Output = T>, future2: impl Future<Output = T>) -> T {
    futures::pin_mut!(future1);
    futures::pin_mut!(future2);

    futures::future::select(future1, future2)
        .await
        .factor_first()
        .0
}

/// Fetch the mime for a given path

pub struct App {
    recent: Option<Vec<String>>,
    out: tokio::io::Stdout,
    matcher: SkimMatcherV2,
    iris: Vec<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            recent: None,
            out: async_stdout(),
            matcher: SkimMatcherV2::default(),
            iris: Vec::new(),
        }
    }
}

#[tokio::main]
pub async fn main() {
    let mut requests = json_input_stream(async_stdin());
    let mut app = App::default();
    app.recent = Some(init());
    while let Some(result) = requests.next().await {
        match result {
            Ok(request) => match request {
                Request::Activate(id) => app.activate(id).await,
                Request::Search(query) => app.search(query).await,
                // Request::Context(id) => app.activate(id).await,
                Request::Exit => break,
                _ => (),
            },
            Err(why) => {
                tracing::error!("malformed JSON input: {}", why);
            }
        }
    }
}

impl App {
    async fn activate(&mut self, id: u32) {
        let selected = &self.iris[id as usize];
        let fid: u32 = selected.split_once("	").unwrap().0.parse().unwrap();
        Command::new("/usr/bin/sh")
            .arg("-c")
            .arg(format!("cliphist decode {fid} | wl-copy"))
            .spawn()
            .unwrap();
        crate::send(&mut self.out, PluginResponse::Close).await;
    }

    async fn search(&mut self, query: String) {
        self.iris.clear();
        if let Some((recent, query)) = self.recent.as_mut().zip(normalized(&query)) {
            let mut recent: Vec<(Option<i64>, &String)> = recent
                .iter()
                .map(|i| (self.matcher.fuzzy_match(i, query.trim()), i))
                .collect();
            recent.sort_by_key(|a| a.0);
            for (score, item) in recent.iter().rev() {
                self.iris.push(item.to_owned().to_owned());
                if score.is_some() {
                    let item = item.replace("	", " ");
                    crate::send(
                        &mut self.out,
                        PluginResponse::Append(PluginSearchResult {
                            id: self.iris.len() as u32 - 1,
                            name: item.to_owned(),
                            // description: self.matcher.fuzzy_match(&item, &query).unwrap().to_string(),
                            icon: Some(IconSource::Mime(Cow::Owned("weather-clear".to_string()))),
                            ..Default::default()
                        }),
                    )
                    .await;
                }
            }
        }
        crate::send(&mut self.out, PluginResponse::Finished).await;
    }
}

fn normalized(input: &str) -> Option<String> {
    input
        .find(' ')
        .map(|pos| input[pos + 1..].trim().to_ascii_lowercase())
}

fn init() -> Vec<String> {
    let bytes = Command::new("cliphist").arg("list").output().unwrap();
    // println!("{:?}",bytes.status.success());
    String::from_utf8(bytes.stdout)
        .unwrap()
        .split('\n')
        .map(|i| i.to_owned())
        .collect()
}

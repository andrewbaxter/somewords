use std::{
    fs::{
        read_dir,
        create_dir_all,
        self,
    },
    collections::HashMap,
    env::current_dir,
    path::PathBuf,
};
use askama::Template;
use chrono::{
    NaiveDateTime,
};
use clap::Parser;
use loga::{
    ea,
    ResultContext,
};

#[cfg(test)]
mod test_colors {
    fn test_oklab() { }
}

fn main() {
    fn inner() -> Result<(), loga::Error> {
        let log = &loga::Log::new(loga::Level::Info);
        let root = current_dir()?;
        let repo = gix::open(&root)?;
        let doc_dir = root.clone();

        // Command line args
        #[derive(Parser)]
        pub struct Args {
            /// Title of blog, suffixed with `-` to document titles
            pub title: String,
            /// Prefix URL for commits; commit hash will be appended for commit links
            pub repo: String,
        }

        let args = Args::parse();

        // Prep file histories from git
        struct HistoryEntry {
            created_time: NaiveDateTime,
            created_hash: String,
            updated_time: NaiveDateTime,
            updated_hash: String,
        }

        let mut history: HashMap<String, HistoryEntry> = HashMap::new();
        for commit in repo.head()?.log_iter().rev()?.into_iter().flatten() {
            let commit = repo.find_object(commit?.new_oid)?.into_commit();
            for entry in commit.tree()?.iter() {
                let entry = entry?;
                match entry.mode() {
                    gix::objs::tree::EntryMode::Blob => {
                        let time =
                            NaiveDateTime::from_timestamp_opt(
                                commit.time()?.seconds_since_unix_epoch.into(),
                                0,
                            ).unwrap();
                        let hash = commit.id.to_hex().to_string();
                        let filename = entry.filename().to_string();
                        match history.entry(filename.clone()) {
                            std::collections::hash_map::Entry::Occupied(mut e) => {
                                let e = e.get_mut();
                                (*e).created_time = time;
                                (*e).created_hash = hash;
                            },
                            std::collections::hash_map::Entry::Vacant(e) => {
                                e.insert(HistoryEntry {
                                    created_time: time.clone(),
                                    created_hash: hash.clone(),
                                    updated_time: time,
                                    updated_hash: hash,
                                });
                            },
                        };
                    },
                    _ => continue,
                }
            }
        }

        // Sort docs and pair with history
        struct DocMeta {
            path: PathBuf,
            filename: String,
            out_filename: String,
            history: HistoryEntry,
        }

        let mut docs = vec![];
        let mut other = vec![];
        let mut copied_style = false;
        for doc in read_dir(&doc_dir).unwrap() {
            let (doc, doc_type) = match doc.and_then(|d| {
                let t = d.file_type()?;
                Ok((d, t))
            }) {
                Ok(d) => d,
                Err(e) => {
                    log.warn_e(e.into(), "Error listing dir element", ea!());
                    continue;
                },
            };
            let filename = doc.file_name().to_string_lossy().to_string();
            if !doc_type.is_file() {
                continue;
            }
            match filename.strip_suffix(".md") {
                Some(short_filename) => {
                    let history = match history.remove(&filename.to_string()) {
                        Some(h) => h,
                        None => {
                            continue;
                        },
                    };
                    docs.push(DocMeta {
                        path: doc.path(),
                        filename: filename.clone(),
                        out_filename: format!("{}.html", short_filename),
                        history: history,
                    });
                },
                None => {
                    if filename == "index.css" {
                        copied_style = true;
                    }
                    other.push(filename);
                },
            };
        }
        docs.sort_by_cached_key(|m| m.history.created_time);
        docs.reverse();

        // Footer?
        let footer = match fs::read(doc_dir.join("footer.md")) {
            Ok(o) => {
                let mut out = String::new();
                pulldown_cmark::html::push_html(
                    &mut out,
                    pulldown_cmark::Parser::new(&String::from_utf8_lossy(&o).to_string()),
                );
                out
            },
            Err(e) => {
                match e.kind() {
                    std::io::ErrorKind::NotFound => "".to_string(),
                    _ => {
                        return Err(e).log_context(log, "Error loading footer", ea!());
                    },
                }
            },
        };

        // Prep out dir and random files
        let out_dir = root.join("pages");
        create_dir_all(&out_dir)?;
        if !copied_style {
            for (
                name,
                bytes,
            ) in [
                ("index.css", include_bytes!("index.css") as &[u8]),
                ("Nunito-VariableFont_wght.ttf", include_bytes!("Nunito-VariableFont_wght.ttf") as &[u8]),
                ("OxygenMono-Regular.ttf", include_bytes!("OxygenMono-Regular.ttf") as &[u8]),
            ] {
                fs::write(
                    out_dir.join(name),
                    bytes,
                ).log_context(log, "Failed to copy built-in file", ea!(name = name))?;
            }
        }
        for other in other {
            fs::copy(
                doc_dir.join(&other),
                out_dir.join(&other),
            ).log_context(log, "Failed to copy over non-doc file", ea!(file = other))?;
        }

        // For each document, generate a page
        for (i, doc) in docs.iter().enumerate() {
            let log = &log.fork(ea!(page = doc.filename));
            let commit_url = format!("{}{}", args.repo, doc.history.updated_hash);
            let old_markdown =
                String::from_utf8(
                    fs::read(&doc.path).log_context(log, "Failed to read markdown file", ea!())?,
                ).log_context(log, "Failed to decode markdown as utf8", ea!())?;
            let mut new_markdown = vec![];
            let mut title = String::new();
            {
                let mut in_h1 = false;
                for e in pulldown_cmark::Parser::new(&old_markdown) {
                    new_markdown.push(e.clone());
                    match e {
                        pulldown_cmark::Event::Start(t) => match t {
                            pulldown_cmark::Tag::Heading(l, _, _) => {
                                match l {
                                    pulldown_cmark::HeadingLevel::H1 => {
                                        in_h1 = true;
                                    },
                                    _ => { },
                                };
                            },
                            _ => { },
                        },
                        pulldown_cmark::Event::End(_) => {
                            if in_h1 {
                                in_h1 = false;
                                new_markdown.push(
                                    pulldown_cmark::Event::Html(
                                        format!(
                                            "<a class=\"timelink\" href=\"{}\">{} {}</a>",
                                            commit_url,
                                            if doc.history.updated_hash == doc.history.created_hash {
                                                "Written"
                                            } else {
                                                "Updated"
                                            },
                                            doc.history.updated_time.date().format("%Y-%m-%d")
                                        ).into(),
                                    ),
                                );
                            }
                        },
                        pulldown_cmark::Event::Text(t) => {
                            if in_h1 {
                                title.push_str(&t);
                            }
                        },
                        pulldown_cmark::Event::Code(t) => {
                            if in_h1 {
                                title.push_str(&t);
                            }
                        },
                        _ => { },
                    }
                }
            };
            let body = {
                let mut body = String::new();
                pulldown_cmark::html::push_html(&mut body, new_markdown.into_iter());
                body
            };

            #[derive(Template)]
            #[template(path = "page.html")]
            struct PageTemplate {
                title: String,
                subtitle: String,
                body: String,
                footer: String,
                front: String,
                fore: String,
                fore_class: String,
                back: String,
                back_class: String,
            }

            let back = docs.get(i + 1);
            fs::write(out_dir.join(&doc.out_filename), PageTemplate {
                title: args.title.clone(),
                subtitle: title.clone(),
                footer: footer.clone(),
                body: body,
                front: docs.get(0).unwrap().out_filename.clone(),
                fore: docs.get(i.saturating_sub(1)).unwrap().out_filename.clone(),
                fore_class: if i == 0 {
                    "disabled_button"
                } else {
                    ""
                }.to_string(),
                back: back.unwrap_or_else(|| doc).out_filename.clone(),
                back_class: if back.is_none() {
                    "disabled_button"
                } else {
                    ""
                }.to_string(),
            }.render().unwrap()).log_context(log, "Failed to write page", ea!())?;
            if i == 0 {
                #[derive(Template)]
                #[template(path = "redirect.html")]
                pub struct RedirectTemplate {
                    title: String,
                    dest: String,
                }

                fs::write(out_dir.join("index.html"), RedirectTemplate {
                    dest: doc.out_filename.clone(),
                    title: args.title.clone(),
                }.render().unwrap()).log_context(log, "Failed to write index", ea!())?;
            }
        }
        return Ok(());
    }

    match inner() {
        Ok(_) => { },
        Err(e) => {
            loga::fatal(e);
        },
    }
}

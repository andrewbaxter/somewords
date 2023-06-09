use std::{
    fs::{
        read_dir,
        create_dir_all,
        self,
    },
    collections::HashMap,
    env::{
        current_dir,
        args,
    },
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
    DebugDisplay,
};
use palette::{
    IntoColor,
    Srgb,
    Oklch,
    OklabHue,
};

fn main() {
    fn inner() -> Result<(), loga::Error> {
        let log = &loga::Log::new(loga::Level::Info);
        let root = current_dir()?;
        let repo =
            gix::open(
                &root,
            ).log_context(log, "Couldn't open git repo in current directory", ea!(cwd = root.to_string_lossy()))?;
        let doc_dir = root.clone();

        // Command line args
        #[derive(Parser)]
        pub struct Args {
            /// Title of blog, suffixed with `-` to document titles
            pub title: String,
            /// Prefix URL for commits; commit hash will be appended for commit links
            pub repo: String,
            /// Represents background color as an angle from 0 (mauve) to 360 (mauve).
            /// Greyscale if not specified.
            #[arg(long)]
            pub color_bg: Option<f32>,
            /// A color relative to the background color if the background color is specified,
            /// otherwise an absolute color.  Defaults to 200 (absolute) or 60 (relative).
            #[arg(long)]
            pub color_accent: Option<f32>,
        }

        let args =
            Args
            ::try_parse().log_context(
                log,
                "Failed to parse command line arguments",
                ea!(argv = args().collect::<Vec<String>>().dbg_str()),
            )?;
        let color_bg = args.color_bg.map(|c| OklabHue::from_degrees(c));
        let color_accent_offset = args.color_accent;

        // Prep file histories from git
        struct HistoryEntry {
            created_time: NaiveDateTime,
            created_hash: String,
            updated_time: NaiveDateTime,
            updated_hash: String,
        }

        let mut history: HashMap<String, HistoryEntry> = HashMap::new();
        for commit in repo
            .head()
            .log_context(log, "Failed to get head commit", ea!())?
            .log_iter()
            .rev()
            .log_context(log, "Error starting commit walk", ea!())?
            .into_iter()
            .flatten() {
            let commit =
                repo
                    .find_object(commit.log_context(log, "Error reading commit in history", ea!())?.new_oid)
                    .log_context(log, "Unable to find object in git corresponding to commit", ea!())?
                    .into_commit();
            for entry in commit.tree().log_context(log, "Error accessing commit tree", ea!())?.iter() {
                let entry = entry.log_context(log, "Error accessing entry in commit tree", ea!())?;
                match entry.mode() {
                    gix::objs::tree::EntryMode::Blob => {
                        let time =
                            NaiveDateTime::from_timestamp_opt(
                                commit
                                    .time()
                                    .log_context(log, "Error accessing commit tree entry time", ea!())?
                                    .seconds_since_unix_epoch
                                    .into(),
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
            let (doc, doc_type) = match doc.log_context(log, "Error reading directory entry", ea!()).and_then(|d| {
                let t =
                    d
                        .file_type()
                        .log_context(
                            log,
                            "Unable to determine directory entry type",
                            ea!(entry = d.file_name().to_string_lossy()),
                        )?;
                Ok((d, t))
            }) {
                Ok(d) => d,
                Err(e) => {
                    log.warn_e(e.into(), "Error listing dir element", ea!());
                    continue;
                },
            };
            let filename = doc.file_name().to_string_lossy().to_string();
            let log = &log.fork(ea!(file = filename));
            if !doc_type.is_file() {
                log.info("Skipping non-file", ea!());
                continue;
            }
            if filename.starts_with(".") {
                log.info("Skipping dot-file", ea!());
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
                        eprintln!("Found {}, won't generate style", filename);
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
        create_dir_all(&out_dir).log_context(log, "Failed to create output directory", ea!())?;
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

            fn color_to_str(c: Oklch<f32>) -> String {
                let c: Srgb<f32> = c.into_color();
                return format!("rgb({}%, {}%, {}%)", c.red * 100., c.green * 100., c.blue * 100.);
            }

            let mut c_h_border = Oklch {
                l: 0.80465806,
                chroma: 0.,
                hue: OklabHue::from_degrees(0.0),
            };
            let mut c_background = Oklch {
                l: 0.99,
                chroma: 0.,
                hue: OklabHue::from_degrees(0.),
            };
            let mut c_code_background = Oklch {
                l: 0.92797065,
                chroma: 0.,
                hue: OklabHue::from_degrees(0.),
            };
            let c_button_background_hover = Oklch {
                l: 0.9940482,
                chroma: 0.,
                hue: OklabHue::from_degrees(0.0),
            };
            let mut c_link = Oklch {
                l: 0.4516303,
                chroma: 0.084099874,
                hue: OklabHue::from_degrees(200.),
            };
            let mut c_link_hover = Oklch {
                l: 0.6,
                chroma: 0.13,
                hue: OklabHue::from_degrees(200.),
            };
            let mut c_date_link = Oklch {
                l: 0.52934384,
                chroma: 0.046049066,
                hue: OklabHue::from_degrees(200.),
            };
            let c_text_light = Oklch {
                l: 0.48193148,
                chroma: 0.,
                hue: OklabHue::from_degrees(0.0),
            };
            let c_code = Oklch {
                l: 0.30118382,
                chroma: 0.,
                hue: OklabHue::from_degrees(0.),
            };
            if let Some(c) = color_bg {
                c_background.chroma = 0.005;
                c_background.hue = c;
                c_code_background.chroma = 0.01;
                c_code_background.hue = c;
                c_h_border.chroma = 0.005;
                c_h_border.hue = c;
            }
            let color_accent = OklabHue::from_degrees(match (color_bg, color_accent_offset) {
                (None, None) => 200.,
                (None, Some(a)) => a,
                (Some(a), None) => a.into_degrees() + 60.,
                (Some(b), Some(r)) => b.into_degrees() + r,
            });
            c_link.hue = color_accent;
            c_link_hover.hue = color_accent;
            c_date_link.hue = color_accent;
            let mut css = include_str!("../templates/index.css").to_string();
            for (
                k,
                replacement,
            ) in [
                ("'^c_h_border^'", color_to_str(c_h_border)),
                ("'^c_background^'", color_to_str(c_background)),
                ("'^c_code_back^'", color_to_str(c_code_background)),
                ("'^c_button_background_hover^'", color_to_str(c_button_background_hover)),
                ("'^c_link^'", color_to_str(c_link)),
                ("'^c_link_hover^'", color_to_str(c_link_hover)),
                ("'^c_date_link^'", color_to_str(c_date_link)),
                ("'^c_text_light^'", color_to_str(c_text_light)),
                ("'^c_code^'", color_to_str(c_code)),
            ] {
                css = css.replace(k, &replacement);
            }
            fs::write(out_dir.join("index.css"), &css).log_context(log, "Failed to write style", ea!())?;
        }
        for other in other {
            let log = &log.fork(ea!(file = other));
            log.info("Copying non-markdown file", ea!());
            fs::copy(
                doc_dir.join(&other),
                out_dir.join(&other),
            ).log_context(log, "Failed to copy over non-doc file", ea!(file = other))?;
        }

        // For each document, generate a page
        for (i, doc) in docs.iter().enumerate() {
            let log = &log.fork(ea!(page = doc.filename));
            log.info("Generating html from markdown file", ea!());
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

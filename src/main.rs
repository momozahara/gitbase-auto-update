use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{ErrorCode, FetchOptions, RemoteCallbacks, Repository};
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use quick_xml::de;
use serde::Deserialize;
use std::fmt::Write;
use std::fs;
use std::{cmp::min, path::Path};

#[derive(Deserialize, Clone)]
struct Settings {
    url: String,
    path: String,
    branch: String,
}

fn spawn_progress_bar() -> ProgressBar {
    let pb = ProgressBar::new(0);
    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-"));
    pb
}

fn run(settings: Settings) -> Result<Repository, git2::Error> {
    let mut repo = match Repository::open(&settings.path) {
        Ok(repo) => Some(repo),
        Err(_) => None,
    };

    if repo.is_none() == false {
        return Ok(repo.unwrap());
    }

    let pb = spawn_progress_bar();

    let mut cb = RemoteCallbacks::new();
    cb.transfer_progress(|stats| {
        let stats_binding = Some(stats.to_owned());
        let stats = stats_binding.as_ref().unwrap();
        pb.set_length(stats.total_objects() as u64);
        let position = min(stats.received_objects(), stats.total_objects());
        pb.set_position(position as u64);
        true
    });

    println!(
        "Cloning repository from '{}' into directory '{}'. Please wait...",
        settings.url, settings.path
    );

    let mut fo = FetchOptions::new();
    fo.depth(1);
    fo.remote_callbacks(cb);
    repo = match RepoBuilder::new()
        .branch(&settings.branch)
        .fetch_options(fo)
        .clone(&settings.url, Path::new(&settings.path))
    {
        Ok(repo) => Some(repo),
        Err(ref e) if e.code() == ErrorCode::NotFound => {
            panic!(
                "Could not clone repository from '{}' branch '{}' does not existed.",
                settings.url, settings.branch
            )
        }
        Err(e) => return Err(e),
    };

    pb.finish();

    Ok(repo.unwrap())
}

fn main() {
    let xml = fs::read_to_string("./settings.xml").unwrap();

    let settings: Settings = de::from_str(&xml).unwrap();

    if settings.url.is_empty() || settings.path.is_empty() {
        panic!("url or path could not be empty");
    }

    match run(settings.clone()) {
        Ok(repo) => {
            let mut remote = repo.find_remote("origin").unwrap();

            remote
                .fetch(&[format!("refs/head/{}", settings.branch)], None, None)
                .unwrap();

            let origin_head = repo.find_reference("refs/remotes/origin/HEAD").unwrap();
            let origin_commit = repo.reference_to_annotated_commit(&origin_head).unwrap();

            // Find the local branch
            let local_branch = repo
                .find_branch(&settings.branch, git2::BranchType::Local)
                .unwrap();
            let local_oid = local_branch.get().target().unwrap();

            // Get the commit for 'origin/HEAD'
            let origin_oid = origin_commit.id();
            let origin_commit = repo.find_commit(origin_oid).unwrap();

            if local_oid == origin_oid {
                println!("Already up to date");
            } else {
                println!("Resetting local '{}' to 'origin/HEAD'...", settings.branch);

                let pb = spawn_progress_bar();

                let mut cb = CheckoutBuilder::new();
                cb.progress(|_, cur, total| {
                    pb.set_length(total as u64);
                    let position = min(cur, total);
                    pb.set_position(position as u64);
                });

                repo.reset(
                    origin_commit.as_object(),
                    git2::ResetType::Hard,
                    Some(&mut cb),
                )
                .unwrap();

                pb.finish();

                println!("Local branch reset to 'origin/HEAD'");
            }
            println!(
                "Current HEAD at commit {}: {}",
                origin_commit.id(),
                origin_commit
                    .message()
                    .unwrap_or("No commit message")
                    .trim()
            );
        }
        Err(e) => println!("error: {}", e),
    };
}

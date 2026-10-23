//! `rill review open`: a full-screen review of one change: its files and
//! their diffs, the threads, and keys to comment, approve, reject and
//! resolve threads.

mod state;
mod view;

use rill_sdk::{DiffFormat, NewComment, RepoClient, Root};
use rill_types::{ChangeId, ReviewStatus, ThreadAnchor, Tree};

use crate::commands::diff::root_diff;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::diff::{render, DiffStyle};
use crate::output::PatchFile;
use crate::Result;
use state::{Action, App, ThreadRow};
use view::Header;

/// Everything the screen shows of a review.
struct Loaded {
    header: Header,
    version: rill_types::ChangeVersionHash,
    files: Vec<PatchFile>,
    threads: Vec<ThreadRow>,
}

async fn load(repo: &RepoClient, id: ChangeId) -> Result<Loaded> {
    let info = repo.reviews().get(id).await?;
    let review = info.review;
    let version = *review
        .versions
        .last()
        .ok_or_else(|| CliError::usage("the change has no versions"))?;
    let cv = repo.change_version(version).await?;
    let base = match cv.parents.first() {
        Some(p) => Root::Version(*p),
        None => Root::Tree(Tree::empty_hash()),
    };
    let (_, files) = root_diff(
        repo,
        &base,
        &Root::Version(version),
        vec![],
        3,
        DiffFormat::Unified,
    )
    .await?;
    let approvals = review
        .votes
        .iter()
        .filter(|v| v.label == "Review" && v.value > 0)
        .count();
    let status = format!(
        "{} · risk {:?} · {} approval{} · {} thread{}",
        match review.status {
            ReviewStatus::Open => "open",
            ReviewStatus::Landable => "landable",
            ReviewStatus::Landing => "landing",
            ReviewStatus::Landed => "landed",
            ReviewStatus::Abandoned => "abandoned",
        },
        review.risk,
        approvals,
        if approvals == 1 { "" } else { "s" },
        review.threads.len(),
        if review.threads.len() == 1 { "" } else { "s" },
    )
    .to_lowercase();
    let threads = review
        .threads
        .iter()
        .map(|t| {
            let at = match &t.anchor {
                ThreadAnchor::Change => "change".to_string(),
                ThreadAnchor::File { path, .. } => path.to_string(),
                ThreadAnchor::Lines { path, start, .. } => format!("{path}:{start}"),
            };
            let first = t
                .comments
                .first()
                .map(|c| format!("{}: {}", c.author, c.body.lines().next().unwrap_or("")))
                .unwrap_or_default();
            ThreadRow {
                id: t.id.clone(),
                resolved: t.resolved,
                text: format!("{at}  {first}"),
            }
        })
        .collect();
    Ok(Loaded {
        header: Header {
            title: format!("{}  {}", id, cv.message.lines().next().unwrap_or("")),
            status,
        },
        version,
        files,
        threads,
    })
}

/// Run the screen for change `id`.
pub async fn open(ctx: &mut Ctx<'_>, id: ChangeId) -> Result<()> {
    if !ctx.interactive() || !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Err(CliError::usage(
            "`rill review open` needs a terminal; try `rill review show`",
        ));
    }
    let repo = ctx.repo()?;
    let mut loaded = load(&repo, id).await?;
    let mut app = App::new(
        loaded.files.iter().map(|f| f.path.clone()).collect(),
        loaded.threads.clone(),
    );
    let theme = crate::output::Theme::with_overrides(&ctx.settings().color.roles);
    let syntax = ctx.settings().diff.syntax;
    let syntax_theme = ctx
        .settings()
        .diff
        .theme
        .clone()
        .unwrap_or_else(|| crate::output::diff::DEFAULT_SYNTAX_THEME.into());
    let diff_text = |files: &[PatchFile], at: usize| -> String {
        let style = DiffStyle {
            color: true,
            theme: &theme,
            syntax,
            syntax_theme: &syntax_theme,
        };
        files
            .get(at)
            .map(|f| {
                if f.binary {
                    "Binary files differ".to_string()
                } else {
                    render(&style, &f.path, &f.unified).join("\n")
                }
            })
            .unwrap_or_default()
    };
    let handle = tokio::runtime::Handle::current();
    let result = tokio::task::block_in_place(|| -> Result<()> {
        let mut terminal = ratatui::init();
        let outcome = (|| -> Result<()> {
            loop {
                let diff = diff_text(&loaded.files, app.selected);
                terminal.draw(|frame| view::draw(frame, &app, &loaded.header, &diff))?;
                let crossterm::event::Event::Key(key) = crossterm::event::read()? else {
                    continue;
                };
                if key.kind != crossterm::event::KeyEventKind::Press {
                    continue;
                }
                let action = app.key(key);
                let done = handle.block_on(async {
                    let reviews = repo.reviews();
                    let outcome: Result<Option<String>> = match action {
                        Action::None => Ok(None),
                        Action::Quit => return Ok::<bool, CliError>(true),
                        Action::Approve => reviews
                            .vote(id, "Review", 2, None, None)
                            .await
                            .map(|_| Some("approved (Review +2)".into()))
                            .map_err(Into::into),
                        Action::Reject => reviews
                            .vote(id, "Review", -2, None, None)
                            .await
                            .map(|_| Some("rejected (Review -2)".into()))
                            .map_err(Into::into),
                        Action::Comment { path, body } => reviews
                            .comment(
                                id,
                                NewComment {
                                    anchor: ThreadAnchor::File {
                                        version: loaded.version,
                                        path: rill_types::RepoPathBuf::new(&path)?,
                                    },
                                    body,
                                    category: None,
                                    confidence: None,
                                    to: None,
                                    suggestion: None,
                                },
                            )
                            .await
                            .map(|_| Some(format!("commented on {path}")))
                            .map_err(Into::into),
                        Action::Resolve {
                            id: thread,
                            resolved,
                        } => reviews
                            .resolve_thread(id, &thread, resolved)
                            .await
                            .map(|_| {
                                Some(
                                    if resolved {
                                        "thread resolved"
                                    } else {
                                        "thread reopened"
                                    }
                                    .into(),
                                )
                            })
                            .map_err(Into::into),
                    };
                    match outcome {
                        Ok(Some(notice)) => {
                            loaded = load(&repo, id).await?;
                            app.threads = loaded.threads.clone();
                            app.thread = app.thread.min(app.threads.len().saturating_sub(1));
                            app.notice = notice;
                        }
                        Ok(None) => {}
                        Err(e) => app.notice = format!("failed: {e}"),
                    }
                    Ok(false)
                })?;
                if done {
                    return Ok(());
                }
            }
        })();
        ratatui::restore();
        outcome
    });
    result
}

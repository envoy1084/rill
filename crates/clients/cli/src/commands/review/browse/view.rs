//! Drawing the review screen: a header with the review's state, the file
//! list beside the shown file's diff, the threads, and a key line.

use ansi_to_tui::IntoText;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use super::state::{App, Focus, Mode};

/// What the header says.
pub struct Header {
    pub title: String,
    pub status: String,
}

/// Draw one frame. `diff` is the shown file's diff as ANSI text.
pub fn draw(frame: &mut Frame<'_>, app: &App, header: &Header, diff: &str) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(if app.threads.is_empty() { 0 } else { 7 }),
            Constraint::Length(1),
        ])
        .split(frame.area());
    let title = Line::from(vec![
        Span::styled(&header.title, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(&header.status, Style::default().fg(Color::Yellow)),
    ]);
    frame.render_widget(Paragraph::new(title), rows[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(28), Constraint::Percentage(72)])
        .split(rows[1]);
    let focus_style = |f: Focus| {
        if app.focus == f {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        }
    };
    let files: Vec<ListItem<'_>> = app
        .files
        .iter()
        .map(|f| ListItem::new(f.as_str()))
        .collect();
    let mut file_state = ListState::default().with_selected(Some(app.selected));
    frame.render_stateful_widget(
        List::new(files)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("files")
                    .border_style(focus_style(Focus::Files)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        body[0],
        &mut file_state,
    );
    let text: Text<'_> = diff
        .into_text()
        .unwrap_or_else(|_| Text::raw(diff.to_string()));
    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(app.file().unwrap_or("")),
            )
            .scroll((app.scroll, 0)),
        body[1],
    );

    if !app.threads.is_empty() {
        let items: Vec<ListItem<'_>> = app
            .threads
            .iter()
            .map(|t| {
                let tag = if t.resolved {
                    Span::styled("resolved ", Style::default().fg(Color::Green))
                } else {
                    Span::styled("open     ", Style::default().fg(Color::Yellow))
                };
                ListItem::new(Line::from(vec![tag, Span::raw(t.text.as_str())]))
            })
            .collect();
        let mut thread_state = ListState::default().with_selected(Some(app.thread));
        frame.render_stateful_widget(
            List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("threads")
                        .border_style(focus_style(Focus::Threads)),
                )
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
            rows[2],
            &mut thread_state,
        );
    }

    let footer = match &app.mode {
        Mode::Comment(text) => Line::from(vec![
            Span::styled("comment: ", Style::default().fg(Color::Cyan)),
            Span::raw(text.as_str()),
            Span::styled("▏  enter send · esc cancel", Style::default().fg(Color::DarkGray)),
        ]),
        Mode::ConfirmReject => Line::from(Span::styled(
            "reject this change? y / n",
            Style::default().fg(Color::Red),
        )),
        Mode::Browse if !app.notice.is_empty() => Line::from(app.notice.as_str()),
        Mode::Browse => Line::from(Span::styled(
            "j/k move · space/b page · J/K line · tab threads · enter resolve · c comment · a approve · r reject · q quit",
            Style::default().fg(Color::DarkGray),
        )),
    };
    frame.render_widget(Paragraph::new(footer), rows[3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::review::browse::state::ThreadRow;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn screen(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
        let header = Header {
            title: "ch_1234 fix parser".into(),
            status: "open · 1 approval".into(),
        };
        terminal
            .draw(|f| draw(f, app, &header, "@@ -1 +1 @@\n-old\n+new"))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_screen_shows_files_the_diff_threads_and_keys() {
        let mut app = App::new(
            vec!["src/a.rs".into(), "b.txt".into()],
            vec![ThreadRow {
                id: "t1".into(),
                resolved: false,
                text: "src/a.rs  ann: why?".into(),
            }],
        );
        let text = screen(&app);
        assert!(text.contains("ch_1234 fix parser"), "{text}");
        assert!(text.contains("src/a.rs") && text.contains("b.txt"));
        assert!(text.contains("+new"));
        assert!(text.contains("ann: why?"));
        assert!(text.contains("a approve"));
        app.mode = Mode::Comment("looks good".into());
        assert!(screen(&app).contains("comment: looks good"));
    }
}

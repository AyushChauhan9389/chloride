//! Interactive terminal UI (`cl tui`).

use anyhow::Result;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
};

use crate::app::{App, AuthForm, AuthKind, InputKind, Mode, View};
use crate::{api, ui};

/// Set up the terminal, run the event loop, and restore on exit.
pub fn launch(auth: Option<AuthKind>) -> Result<()> {
    let terminal = ratatui::init();
    let mut app = App::new();
    if let Some(kind) = auth {
        app.mode = Mode::Auth(AuthForm::new(kind));
    }
    let result = run(terminal, app);
    ratatui::restore();
    result
}

fn run(mut terminal: DefaultTerminal, mut app: App) -> Result<()> {
    while app.running {
        terminal.draw(|frame| ui::render(frame, &app))?;

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            on_key(&mut app, key);
        }
    }
    Ok(())
}

fn on_key(app: &mut App, key: KeyEvent) {
    // Take the current mode out so we can freely mutate `app`; default back to Browse.
    let mode = std::mem::replace(&mut app.mode, Mode::Browse);

    match mode {
        Mode::Browse => on_browse_key(app, key.code),
        Mode::Input { kind, buffer } => on_input_key(app, key.code, kind, buffer),
        Mode::Confirm { name, is_dir } => on_confirm_key(app, key.code, name, is_dir),
        Mode::ConfirmRemoteDelete { id, name } => on_confirm_remote_delete_key(app, key.code, id, name),
        Mode::Message { .. } => {
            // Any key dismisses the message popup.
        }
        Mode::Auth(form) => on_auth_key(app, key.code, form),
        Mode::Quota(_) => {
            // Any key dismisses the quota overlay.
        }
    }
}

fn on_browse_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => app.running = false,
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Home | KeyCode::Char('g') => app.select_first(),
        KeyCode::End | KeyCode::Char('G') => app.select_last(),
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => app.enter(),
        KeyCode::Char('R') if app.view == View::Files => app.regenerate_selected_file_url(),
        KeyCode::Char('c') if app.view == View::Files => app.copy_selected_short_download_url(),
        KeyCode::Char('D') if app.view == View::Files => app.download_selected_file(),
        KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => app.go_up(),
        KeyCode::Char('r') => {
            app.refresh();
        }
        // Local file-manager operations only apply in FileManager view.
        KeyCode::Char('t') if app.view == View::FileManager => {
            app.mode = Mode::Input {
                kind: InputKind::Touch,
                buffer: String::new(),
            };
        }
        KeyCode::Char('m') if app.view == View::FileManager => {
            app.mode = Mode::Input {
                kind: InputKind::Mkdir,
                buffer: String::new(),
            };
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if app.view == View::Files {
                app.confirm_delete_selected_remote_file();
            } else if app.view == View::FileManager {
                if let Some(entry) = app.selected_entry()
                    && !entry.parent
                {
                    app.mode = Mode::Confirm {
                        name: entry.name.clone(),
                        is_dir: entry.is_dir,
                    };
                }
            }
        }
        // Toggle between remote files view and local file manager.
        KeyCode::Char('f') => {
            let next = match app.view {
                View::Files => View::FileManager,
                View::FileManager => View::Files,
            };
            app.switch_view(next);
        }
        KeyCode::Char('L') => {
            // Always open the login form. If we have a remembered email,
            // pre-fill it so re-login after token expiry is quick.
            let mut form = AuthForm::new(AuthKind::Login);
            if let Some(user) = &app.config.user {
                form.email = user.email.clone();
                form.active = 1; // jump straight to password
            }
            app.mode = Mode::Auth(form);
        }
        KeyCode::Char('S') => {
            app.mode = Mode::Auth(AuthForm::new(AuthKind::Register));
        }
        KeyCode::Char('u') => {
            match api::get_storage(&mut app.config) {
                Ok(info) => app.mode = Mode::Quota(Some(Ok(info))),
                Err(e) => app.mode = Mode::Quota(Some(Err(e.to_string()))),
            }
        }
        _ => {}
    }
}

fn on_input_key(app: &mut App, code: KeyCode, kind: InputKind, mut buffer: String) {
    match code {
        KeyCode::Esc => {} // cancel: mode already reset to Browse
        KeyCode::Enter => match kind {
            InputKind::Touch => app.create_file(&buffer),
            InputKind::Mkdir => app.create_dir(&buffer),
        },
        KeyCode::Backspace => {
            buffer.pop();
            app.mode = Mode::Input { kind, buffer };
        }
        KeyCode::Char(c) => {
            buffer.push(c);
            app.mode = Mode::Input { kind, buffer };
        }
        _ => {
            // Keep editing on any other key.
            app.mode = Mode::Input { kind, buffer };
        }
    }
}

fn on_confirm_key(app: &mut App, code: KeyCode, name: String, is_dir: bool) {
    if matches!(code, KeyCode::Char('y') | KeyCode::Char('Y')) {
        app.delete(&name, is_dir);
    }
    // otherwise cancel: mode already reset to Browse
}

fn on_confirm_remote_delete_key(app: &mut App, code: KeyCode, id: i64, name: String) {
    if matches!(code, KeyCode::Char('y') | KeyCode::Char('Y')) {
        app.delete_remote_file(id, &name);
    }
}

fn on_auth_key(app: &mut App, code: KeyCode, mut form: AuthForm) {
    match code {
        KeyCode::Esc => {} // cancel: mode already reset to Browse
        KeyCode::Tab | KeyCode::Down => {
            form.active = (form.active + 1) % form.field_count();
            app.mode = Mode::Auth(form);
        }
        KeyCode::Up => {
            form.active = if form.active == 0 {
                form.field_count() - 1
            } else {
                form.active - 1
            };
            app.mode = Mode::Auth(form);
        }
        KeyCode::Backspace => {
            form.active_buffer().pop();
            app.mode = Mode::Auth(form);
        }
        KeyCode::Char(c) => {
            form.active_buffer().push(c);
            app.mode = Mode::Auth(form);
        }
        KeyCode::Enter => submit_auth(app, form),
        _ => {
            app.mode = Mode::Auth(form);
        }
    }
}

fn submit_auth(app: &mut App, mut form: AuthForm) {
    if form.email.trim().is_empty() {
        form.error = Some("Email is required".into());
        app.mode = Mode::Auth(form);
        return;
    }
    if form.password.is_empty() {
        form.error = Some("Password is required".into());
        app.mode = Mode::Auth(form);
        return;
    }

    let result = match form.kind {
        AuthKind::Login => api::login(&form.email, &form.password, &app.config.base_url),
        AuthKind::Register => {
            if form.password != form.confirm {
                form.error = Some("Passwords do not match".into());
                app.mode = Mode::Auth(form);
                return;
            }
            api::register(&form.email, &form.password, &app.config.base_url)
        }
    };

    match result {
        Ok(auth) => {
            let email = auth.user.email.clone();
            app.config.token = Some(auth.token);
            app.config.refresh_token = Some(auth.refresh_token);
            app.config.user = Some(auth.user);
            let _ = app.config.save();
            app.set_success(format!("✓ Logged in as {email}"));
            // mode already reset to Browse
        }
        Err(e) => {
            form.error = Some(e.to_string());
            app.mode = Mode::Auth(form);
        }
    }
}

//! rmap-config: settings window (Slint GUI). Edits the fields that exist in
//! `data/config.json` (rmap_core::config::AppConfig) and writes them back.
//!
//! Usage:
//!   rmap-config            -> open settings window
//!   rmap-config reload     -> send IPC reload to the running daemon (CLI helper)
//!   rmap-config status     -> (stub)
//!   rmap-config quit       -> (stub)

use anyhow::Result;
use rmap_core::config::AppConfig;
use rmap_core::ipc::{send_command, send_reload_command, IpcCommand, IpcResponse};
use slint::{Model, VecModel};
use std::path::Path;
use std::rc::Rc;

slint::slint! {
    import { Button, CheckBox, LineEdit, ComboBox, VerticalBox, HorizontalBox, ScrollView } from "std-widgets.slint";

    export struct ProfileRow {
        name: string,
        layout: string,
        sands: bool,
        gestures: bool,
        shortcuts: bool,
    }

    component CategoryItem inherits Rectangle {
        in property <string> label;
        in property <bool> selected;
        callback clicked();

        height: 36px;
        background: selected ? #3a3a50 : transparent;

        Text {
            text: label;
            x: 10px;
            vertical-alignment: center;
            color: selected ? white : black;
        }

        TouchArea {
            clicked => { root.clicked(); }
        }
    }

    export component AppWindow inherits Window {
        title: "rmap 設定";
        preferred-width: 680px;
        preferred-height: 480px;

        in-out property <int> category_index: 0;
        in-out property <string> default_layout;
        in-out property <int> default_profile_index: 0;
        in-out property <bool> enable_log;
        in-out property <bool> disable_ctrl;
        in-out property <bool> disable_alt;
        in-out property <bool> disable_win;
        in-out property <bool> disable_shift;
        in-out property <[ProfileRow]> profiles;
        in property <[string]> profile_names;
        in-out property <int> current_profile_index: 0;
        in property <string> status_text;
        in property <string> running_status_text;
        in property <bool> daemon_suspended;
        in property <string> current_profile_text;
        in property <string> current_layout_text;

        callback save();
        callback toggle_running();
        callback restart();
        callback quit();
        callback browse_default_layout();
        callback browse_profile_layout();
        callback profile_layout_edited(string);
        callback toggle_profile_sands();
        callback toggle_profile_gestures();
        callback toggle_profile_shortcuts();

        HorizontalBox {
            padding: 0px;
            spacing: 0px;

            // Left pane: category list (flush, no gaps between items).
            VerticalBox {
                width: 160px;
                padding: 0px;
                spacing: 0px;
                CategoryItem {
                    label: "全般";
                    selected: category_index == 0;
                    clicked => { category_index = 0; }
                }
                CategoryItem {
                    label: "キー無効化";
                    selected: category_index == 1;
                    clicked => { category_index = 1; }
                }
                CategoryItem {
                    label: "ログ";
                    selected: category_index == 2;
                    clicked => { category_index = 2; }
                }
                CategoryItem {
                    label: "プロファイル";
                    selected: category_index == 3;
                    clicked => { category_index = 3; }
                }
                CategoryItem {
                    label: "デーモン操作";
                    selected: category_index == 4;
                    clicked => { category_index = 4; }
                }

                Rectangle {}
            }

            // Divider line between the category list and the settings pane.
            Rectangle {
                width: 1px;
                background: #c0c0c0;
            }

            // Right pane: settings for the selected category, with a
            // save bar pinned to the bottom regardless of scroll position.
            VerticalBox {
                padding: 0px;
                spacing: 0px;

                ScrollView {
                    VerticalBox {
                        if category_index == 0 : VerticalBox {
                            Text { text: "全般"; font-weight: 700; }
                            HorizontalBox {
                                Text { text: "デフォルトレイアウト:"; vertical-alignment: center; }
                                LineEdit { text <=> default_layout; }
                                Button { text: "📁"; clicked => { browse_default_layout(); } }
                            }
                            HorizontalBox {
                                Text { text: "デフォルトプロファイル:"; vertical-alignment: center; }
                                ComboBox {
                                    model: profile_names;
                                    current-index <=> default_profile_index;
                                }
                            }
                        }

                        if category_index == 1 : VerticalBox {
                            Text { text: "一時無効化キー（押している間は全パススルー）"; font-weight: 700; }
                            HorizontalBox {
                                CheckBox { text: "Ctrl"; checked <=> disable_ctrl; }
                                CheckBox { text: "Alt"; checked <=> disable_alt; }
                                CheckBox { text: "Win"; checked <=> disable_win; }
                                CheckBox { text: "Shift"; checked <=> disable_shift; }
                            }
                        }

                        if category_index == 2 : VerticalBox {
                            Text { text: "ログ"; font-weight: 700; }
                            CheckBox { text: "ログを有効にする (実行ファイルと同じフォルダの ./log に出力)"; checked <=> enable_log; }
                        }

                        if category_index == 3 : VerticalBox {
                            Text { text: "プロファイル"; font-weight: 700; }
                            ComboBox {
                                model: profile_names;
                                current-index <=> current_profile_index;
                            }
                            if profiles.length > 0 && current_profile_index >= 0 : VerticalBox {
                                HorizontalBox {
                                    Text { text: "レイアウト:"; vertical-alignment: center; }
                                    LineEdit {
                                        text: profiles[current_profile_index].layout;
                                        edited(text) => { profile_layout_edited(text); }
                                    }
                                    Button { text: "📁"; clicked => { browse_profile_layout(); } }
                                }
                                HorizontalBox {
                                    CheckBox {
                                        text: "SandS";
                                        checked: profiles[current_profile_index].sands;
                                        toggled => { toggle_profile_sands(); }
                                    }
                                    CheckBox {
                                        text: "Gestures";
                                        checked: profiles[current_profile_index].gestures;
                                        toggled => { toggle_profile_gestures(); }
                                    }
                                    CheckBox {
                                        text: "Shortcuts";
                                        checked: profiles[current_profile_index].shortcuts;
                                        toggled => { toggle_profile_shortcuts(); }
                                    }
                                }
                            }
                        }

                        if category_index == 4 : VerticalBox {
                            vertical-stretch: 0;
                            Text { text: "デーモン操作"; font-weight: 700; }
                            HorizontalBox {
                                vertical-stretch: 0;
                                alignment: start;
                                Button {
                                    text: daemon_suspended ? "再生" : "停止";
                                    width: 80px; height: 32px; horizontal-stretch: 0;
                                    clicked => { toggle_running(); }
                                }
                                Button { text: "再起動"; width: 80px; height: 32px; horizontal-stretch: 0; clicked => { restart(); } }
                                Button { text: "終了"; width: 80px; height: 32px; horizontal-stretch: 0; clicked => { quit(); } }
                            }
                        }
                    }
                }

                // Divider line above the always-visible save bar.
                Rectangle {
                    height: 1px;
                    background: #c0c0c0;
                }

                HorizontalBox {
                    VerticalBox {
                        padding: 0px;
                        spacing: 2px;
                        alignment: start;
                        Text { text: "状態: " + running_status_text; wrap: word-wrap; }
                        Text { text: "プロファイル: " + current_profile_text; wrap: word-wrap; }
                        Text { text: "レイアウト: " + current_layout_text; wrap: word-wrap; }
                    }
                    Text { text: status_text; vertical-alignment: center; horizontal-stretch: 1; }
                    Button { text: "保存"; clicked => { save(); } }
                }
            }
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");
    match cmd {
        "reload" => {
            match send_reload_command() {
                Ok(IpcResponse::Ok) => println!("reload sent"),
                Ok(r) => println!("response: {:?}", r),
                Err(e) => eprintln!("IPC error: {e} (is daemon running?)"),
            }
            return Ok(());
        }
        "status" => {
            println!("status: (IPC status not fully wired in prototype; daemon tray shows state)");
            return Ok(());
        }
        "quit" => {
            println!("quit: (send IpcCommand::Quit via pipe in full impl)");
            return Ok(());
        }
        _ => {}
    }

    // Avoid piling up windows when 設定 is pressed repeatedly from the tray:
    // if a settings window is already open, just bring it to front.
    if focus_existing_window() {
        return Ok(());
    }

    run_settings_window()
}

/// Find an already-open settings window by its title and bring it to the
/// foreground. Returns true if such a window was found (and this process
/// should exit without creating a new one).
#[cfg(windows)]
fn focus_existing_window() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    let title: Vec<u16> = "rmap 設定"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let hwnd = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr()));
        if hwnd.0 == 0 {
            return false;
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
        true
    }
}

#[cfg(not(windows))]
fn focus_existing_window() -> bool {
    false
}

const CONFIG_PATH: &str = "data/config.json";

/// Pick a layout file via a native file dialog, starting in `data/layouts` if it exists.
fn pick_layout_file() -> Option<String> {
    let mut dialog = rfd::FileDialog::new().add_filter("Layout", &["txt"]);
    let start_dir = Path::new("data/layouts");
    if start_dir.exists() {
        dialog = dialog.set_directory(start_dir);
    }
    dialog
        .pick_file()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

fn run_settings_window() -> Result<()> {
    let cfg = AppConfig::load(Path::new(CONFIG_PATH)).unwrap_or_else(|_| AppConfig::fallback());

    let window = AppWindow::new()?;

    window.set_default_layout(cfg.default_layout.clone().into());
    window.set_enable_log(cfg.enable_log);

    let lower_disable: Vec<String> = cfg
        .disable_keys
        .iter()
        .map(|s| s.trim().to_lowercase())
        .collect();
    window.set_disable_ctrl(lower_disable.iter().any(|s| s == "ctrl" || s == "control"));
    window.set_disable_alt(lower_disable.iter().any(|s| s == "alt" || s == "menu"));
    window.set_disable_win(
        lower_disable
            .iter()
            .any(|s| matches!(s.as_str(), "win" | "meta" | "super" | "cmd")),
    );
    window.set_disable_shift(lower_disable.iter().any(|s| s == "shift"));

    // Keep custom (non ctrl/alt/win/shift) entries so we round-trip them unchanged.
    let custom_disable_keys: Vec<String> = cfg
        .disable_keys
        .iter()
        .filter(|s| {
            !matches!(
                s.trim().to_lowercase().as_str(),
                "ctrl" | "control" | "alt" | "menu" | "win" | "meta" | "super" | "cmd" | "shift"
            )
        })
        .cloned()
        .collect();

    let mut profile_names: Vec<String> = cfg.profiles.keys().cloned().collect();
    profile_names.sort();
    let rows: Vec<ProfileRow> = profile_names
        .iter()
        .map(|name| {
            let p = &cfg.profiles[name];
            ProfileRow {
                name: name.clone().into(),
                layout: p.layout.clone().into(),
                sands: p.toggles.enable_sands,
                gestures: p.toggles.enable_gestures,
                shortcuts: p.toggles.enable_shortcuts,
            }
        })
        .collect();
    let profiles_model = Rc::new(VecModel::from(rows));
    window.set_profiles(slint::ModelRc::from(profiles_model.clone()));

    let names_model = Rc::new(VecModel::from(
        profile_names
            .iter()
            .map(|n| n.clone().into())
            .collect::<Vec<slint::SharedString>>(),
    ));
    window.set_profile_names(slint::ModelRc::from(names_model));
    window.set_current_profile_index(if profile_names.is_empty() { -1 } else { 0 });

    let default_profile_index = profile_names
        .iter()
        .position(|n| n == &cfg.app_map.default_profile)
        .map(|i| i as i32)
        .unwrap_or(0);
    window.set_default_profile_index(default_profile_index);

    window.set_status_text("".into());
    refresh_profile_layout_text(&window, &cfg);
    refresh_daemon_status(&window);

    // 設定 -> デフォルトレイアウトの参照ファイルを選択ダイアログで指定する。
    let window_weak = window.as_weak();
    window.on_browse_default_layout(move || {
        let window = window_weak.unwrap();
        if let Some(path) = pick_layout_file() {
            window.set_default_layout(path.into());
        }
    });

    // 選択中プロファイルのレイアウトファイルを選択ダイアログで指定する。
    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_browse_profile_layout(move || {
        let window = window_weak.unwrap();
        let idx = window.get_current_profile_index();
        if idx < 0 {
            return;
        }
        let idx = idx as usize;
        if let Some(path) = pick_layout_file() {
            if let Some(mut row) = model.row_data(idx) {
                row.layout = path.into();
                model.set_row_data(idx, row);
            }
        }
    });

    // 選択中プロファイルのレイアウトを直接入力で編集する。
    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_profile_layout_edited(move |text| {
        let window = window_weak.unwrap();
        let idx = window.get_current_profile_index();
        if idx < 0 {
            return;
        }
        let idx = idx as usize;
        if let Some(mut row) = model.row_data(idx) {
            row.layout = text;
            model.set_row_data(idx, row);
        }
    });

    // 選択中プロファイルのトグル（SandS / Gestures / Shortcuts）。
    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_toggle_profile_sands(move || {
        let window = window_weak.unwrap();
        let idx = window.get_current_profile_index();
        if idx < 0 {
            return;
        }
        let idx = idx as usize;
        if let Some(mut row) = model.row_data(idx) {
            row.sands = !row.sands;
            model.set_row_data(idx, row);
        }
    });

    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_toggle_profile_gestures(move || {
        let window = window_weak.unwrap();
        let idx = window.get_current_profile_index();
        if idx < 0 {
            return;
        }
        let idx = idx as usize;
        if let Some(mut row) = model.row_data(idx) {
            row.gestures = !row.gestures;
            model.set_row_data(idx, row);
        }
    });

    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_toggle_profile_shortcuts(move || {
        let window = window_weak.unwrap();
        let idx = window.get_current_profile_index();
        if idx < 0 {
            return;
        }
        let idx = idx as usize;
        if let Some(mut row) = model.row_data(idx) {
            row.shortcuts = !row.shortcuts;
            model.set_row_data(idx, row);
        }
    });

    // デーモン操作（再生/停止トグル／再起動／終了）。デーモンが起動していない
    // 場合は status_text にエラーを表示するだけ（NFR-4 fail-fast）。
    let window_weak = window.as_weak();
    window.on_toggle_running(move || {
        let window = window_weak.unwrap();
        report_ipc_result(&window, send_command(&IpcCommand::ToggleRunning), "切り替えました");
        refresh_daemon_status(&window);
    });

    let window_weak = window.as_weak();
    window.on_quit(move || {
        let window = window_weak.unwrap();
        report_ipc_result(&window, send_command(&IpcCommand::Quit), "終了しました");
        refresh_daemon_status(&window);
    });

    let window_weak = window.as_weak();
    window.on_restart(move || {
        let window = window_weak.unwrap();
        report_ipc_result(
            &window,
            send_command(&IpcCommand::Restart),
            "再起動しました",
        );
        refresh_daemon_status(&window);
    });

    let mut base_cfg = cfg;
    let window_weak = window.as_weak();
    let model = profiles_model.clone();
    window.on_save(move || {
        let window = window_weak.unwrap();

        base_cfg.default_layout = window.get_default_layout().to_string();
        let idx = window.get_default_profile_index();
        if let Some(name) = window.get_profile_names().iter().nth(idx.max(0) as usize) {
            base_cfg.app_map.default_profile = name.to_string();
        }
        base_cfg.enable_log = window.get_enable_log();

        let mut disable_keys = custom_disable_keys.clone();
        if window.get_disable_ctrl() {
            disable_keys.push("ctrl".to_string());
        }
        if window.get_disable_alt() {
            disable_keys.push("alt".to_string());
        }
        if window.get_disable_win() {
            disable_keys.push("win".to_string());
        }
        if window.get_disable_shift() {
            disable_keys.push("shift".to_string());
        }
        base_cfg.disable_keys = disable_keys;

        for row in model.iter() {
            if let Some(p) = base_cfg.profiles.get_mut(row.name.as_str()) {
                p.layout = row.layout.to_string();
                p.toggles.enable_sands = row.sands;
                p.toggles.enable_gestures = row.gestures;
                p.toggles.enable_shortcuts = row.shortcuts;
            }
        }

        match save_config(&base_cfg) {
            Ok(()) => window.set_status_text("保存しました".into()),
            Err(e) => window.set_status_text(format!("保存に失敗: {e}").into()),
        }
        refresh_profile_layout_text(&window, &base_cfg);
    });

    window.run()?;
    Ok(())
}

/// Reflect the result of an IPC daemon-control command in the status line.
fn report_ipc_result(window: &AppWindow, result: anyhow::Result<IpcResponse>, ok_text: &str) {
    match result {
        Ok(IpcResponse::Ok) => window.set_status_text(ok_text.into()),
        Ok(r) => window.set_status_text(format!("{r:?}").into()),
        Err(e) => window.set_status_text(format!("デーモンに接続できません: {e}").into()),
    }
}

/// Query the daemon's running/suspended state via IPC and update the
/// left-pane status panel. Shows "未起動" if the daemon isn't reachable.
fn refresh_daemon_status(window: &AppWindow) {
    let (text, suspended) = match send_command(&IpcCommand::Status) {
        Ok(IpcResponse::Status { suspended, .. }) => {
            (if suspended { "停止中" } else { "稼働中" }, suspended)
        }
        // Daemon unreachable: show "再生" as the actionable toggle label.
        _ => ("未起動", true),
    };
    window.set_running_status_text(text.into());
    window.set_daemon_suspended(suspended);
}

/// Update the left-pane "current profile / layout" display from `cfg`'s
/// default profile.
fn refresh_profile_layout_text(window: &AppWindow, cfg: &AppConfig) {
    let profile = cfg.app_map.default_profile.clone();
    let layout = cfg
        .profiles
        .get(&profile)
        .map(|p| p.layout.clone())
        .unwrap_or_else(|| cfg.default_layout.clone());
    window.set_current_profile_text(profile.into());
    window.set_current_layout_text(layout.into());
}

fn save_config(cfg: &AppConfig) -> Result<()> {
    let json = serde_json::to_string_pretty(cfg)?;
    if let Some(parent) = Path::new(CONFIG_PATH).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(CONFIG_PATH, json)?;
    Ok(())
}

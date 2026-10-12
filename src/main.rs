// GUI app: no console window in release builds (debug keeps the terminal
// for eventual stderr output).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cloud;
mod config;
mod crypto;
mod router;
mod styles;

use cloud::*;
use config::Config;
use iced::widget::{button, column, container, mouse_area, pick_list, row, scrollable, space, text,
    text_input};
use iced::{Background, Border, Color, Length, Task, Theme, Widget, border::Radius};
use router::RouterStatus;

// Embedded icon fonts (Font Awesome Free, CC BY 4.0) so device types and
// the game row get brand icons (android robot, apple, windows logo, steam,
// gamepad…). Registered via `Application::fonts` in main(); referenced via
// the `Font::new` family below.
static FA_BRANDS_BYTES: &[u8] = include_bytes!("../assets/fa-brands-400.ttf");
const FA_BRANDS_FONT: iced::Font = iced::Font::new("Font Awesome 6 Brands");
/// Windows 10/11 bundled symbol font (for toolbar icon buttons):
const MDL2_FONT: iced::Font = iced::Font::new("Segoe MDL2 Assets");

fn main() -> iced::Result {
    // GUI-subsystem builds lose the console: route panics to a file so
    // crashes are diagnosable (panic=abort still invokes the hook).
    std::panic::set_hook(Box::new(|info| {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let msg = format!(
            "panic @{ts}: {info}\n{:#?}\n",
            std::backtrace::Backtrace::force_capture(),
        );
        let path = std::env::temp_dir().join("uugui-panic.log");
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        if let Some(mut log) = log {
            let _ = log.write_all(msg.as_bytes());
        }
        use std::io::Write as _;
        let _ = std::io::stderr().write_all(msg.as_bytes());
    }));
    #[cfg(windows)]
    unsafe {
        #[link(name = "user32")]
        extern "system" {
            fn SetProcessDpiAwarenessContext(value: isize) -> i32;
        }
        // Native-rendering DPI context. PMv2 (-4) is the correct choice on
        // up-to-date winit/iced: the app renders at the monitor's native DPI
        // (crisp text) and sizes the window correctly.
        let _ = SetProcessDpiAwarenessContext(-4);
    }
    let mut app = iced::application(boot, update, view)
        .title("UU主机加速器PC版")
        .theme(theme)
        .fonts([FA_BRANDS_BYTES])
        // The window is created DECORATED but INVISIBLE: the native frame
        // (resize borders + DWM shadow) exists from the start, and the
        // one-shot boot hook strips the caption before the window is ever
        // shown — so the user never sees a borderless-no-shadow (or title
        // bar) intermediate state. NOTE: `.window()` re-places the settings,
        // so `.window_size()` must run AFTER it.
        .window(iced::window::Settings {
            visible: false,
            ..iced::window::Settings::default()
        })
        .resizable(true)
        .window_size((1000.0, 720.0))
        .subscription(|app| {
            let mut subs = Vec::new();
            // Tick ONLY while a toast is entering (or exiting), and right
            // after the TTL lapses (to start the exit). The static display
            // phase gets NO ticks: a toast idling on screen would otherwise
            // force a full view+layout rebuild every 16ms, making scrolling
            // laggy. The exit itself is re-armed by the one-shot wake-up
            // scheduled in `update` when the last entry tick fires.
            let active = app.toasts.iter().any(|t| match t.closing {
                Some(d) => d.elapsed() < TOAST_SLIDE,
                None => t.at.elapsed() < TOAST_SLIDE,
            });
            if active {
                subs.push(iced::Subscription::run(|| {
                    iced::stream::channel(1, |mut sender: iced::futures::channel::mpsc::Sender<()>| async move {
                        loop {
                            use iced::futures::SinkExt;
                        let _ = sender.send(()).await;
                            std::thread::sleep(TOAST_TICK);
                        }
                    })
                }).map(|_| Message::ToastTick));
            }
            iced::Subscription::batch(subs)
        });
    // 0.15: fonts are no longer discovered from the system; load the CJK
    // faces from disk, register them as binaries and make the family the
    // application default. Icon fonts keep their explicit `Font::new` refs.
    if let Some((cjk_bytes, family)) = load_cjk_font() {
        FONT_FAMILY.set(family).ok();
        let mut faces: Vec<Vec<u8>> = vec![cjk_bytes];
        let bold = load_cjk_bold();
        if !bold.is_empty() {
            faces.push(bold);
        }
        app = app.fonts(faces).font(iced::Font::new(family));
    }
    app.run()
}

/// 0.14 boot function: builds the initial state and its start-up tasks.
fn boot() -> (App, Task<Message>) {
    let state = App::default();
    let mut t = Task::none();
    // native frame hook: the window is created (hidden) before the event
    // loop starts, so a short delay is only a safety margin.
    t = Task::batch([t, delay(100, || Message::FrameBoost)]);
    if !state.session.is_empty() {
        t = Task::batch([t, Task::done(Message::RefreshAll)]);
    }
    if !state.avatar_url.is_empty() {
        t = Task::batch([t, avatar_task(state.avatar_url.clone())]);
    }
    (state, t)
}

#[repr(C)]
struct NccCalcSizeParams {
    rgrc: [Rect; 3],
    #[allow(non_snake_case)]
    lpfnStatusProc: isize,
    #[allow(non_snake_case)]
    lprcClip: *mut Rect,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

/// Windows: give the undecorated window the native frame behaviors —
/// DWM shadow and system resize borders — WITHOUT a native title bar.
///
/// winit 0.30 implements `decorations(false)` by hijacking `WM_NCCALCSIZE`
/// (the client area covers the whole window) and clearing the caption bits,
/// so simply re-adding `WS_THICKFRAME` does nothing. Instead we ask winit to
/// switch to the decorated state (see `Message::FrameBoost`), then subclass
/// the window procedure to strip the caption from the client area while
/// keeping the frame's resize borders. The app keeps drawing its own header
/// strip; the client area stays flush with the top edge.
///
/// NOTE: DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE, ROUND) is
/// intentionally NOT applied: on Windows 11 it used to cause a DWM redraw
/// loop with the wgpu DX12 swapchain (iced#2926). Win11 rounds
/// WS_THICKFRAME windows by default, so it is not needed.
#[cfg(windows)]
unsafe fn boost_window_frame() {
    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(cb: extern "system" fn(isize, isize) -> i32, param: isize) -> i32;
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
        fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
        fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
        fn IsZoomed(hwnd: isize) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn SetWindowPos(
            hwnd: isize,
            after: isize,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            flags: u32,
        ) -> i32;
        fn CallWindowProcW(
            prev: extern "system" fn(isize, u32, usize, isize) -> isize,
            hwnd: isize,
            msg: u32,
            wparam: usize,
            lparam: isize,
        ) -> isize;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    }

    const GWL_STYLE: i32 = -16;
    const GWLP_WNDPROC: i32 = -4;
    const WS_CAPTION: isize = 0x00C0_0000;
    const WS_THICKFRAME: isize = 0x0004_0000;
    const WS_MINIMIZEBOX: isize = 0x0002_0000;
    const WS_MAXIMIZEBOX: isize = 0x0001_0000;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SW_SHOW: i32 = 5;
    const WM_NCCALCSIZE: u32 = 0x0083;
    const WM_NCHITTEST: u32 = 0x0084;
    const HTCLIENT: isize = 1;
    const HTTOP: isize = 12;
    const SM_CXPADDEDBORDER: i32 = 92;
    const SM_CXSIZEFRAME: i32 = 32;
    const SM_CYSIZEFRAME: i32 = 33;

    /// lparam of WM_HITTEST carries screen coords packed as POINTS; convert
    /// to a screen-space point (signed 16-bit halves).
    unsafe fn hit_test_point(lparam: isize) -> (i32, i32) {
        let x = (lparam & 0xFFFF) as u16 as i16 as i32;
        let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32;
        (x, y)
    }

    /// Current window rect (screen coords) and border thickness. Calls do NOT
    /// use the transitional NCCALCSIZE state, so this is only valid in
    /// NCHITTEST (never called mid-frame-transition).
    unsafe fn window_metrics(hwnd: isize) -> (Rect, i32, i32) {
        let mut win = Rect { left: 0, top: 0, right: 0, bottom: 0 };
        GetWindowRect(hwnd, &mut win);
        let frame_x = GetSystemMetrics(SM_CXSIZEFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
        let frame_y = GetSystemMetrics(SM_CYSIZEFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
        (win, frame_y, frame_x)
    }

    struct Search {
        pid: u32,
        found: isize,
    }

    extern "system" fn find_main_window(hwnd: isize, param: isize) -> i32 {
        unsafe {
            let search = &mut *(param as *mut Search);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid != search.pid {
                return 1; // keep enumerating
            }
            // The main (client) window carries a caption style; helper
            // windows of the winit shell do not.
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            if style & WS_CAPTION == WS_CAPTION {
                search.found = hwnd;
                return 0; // stop
            }
            1
        }
    }

    /// Subclassed window procedure: keeps the native frame (resize borders,
    /// shadow, rounded corners) but removes the caption from the client area.
    extern "system" fn frame_proc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize {
        unsafe {
            match msg {
                WM_NCCALCSIZE if wparam != 0 => {
                    // Define the client area ourselves: the proposed window
                    // rect, minus a thin resize border on left/right/bottom.
                    // The top has no inset, so the custom header strip owns
                    // the caption area.
                    //
                    // NOTE: only RELATIVE insets are applied here. The design
                    // must not re-derive the rect from GetWindowRect: during
                    // maximize/restore transitions the current window rect is
                    // mid-update, and mixing client/screen coordinate spaces
                    // there corrupts the client rect permanently.
                    let params = lparam as *mut NccCalcSizeParams;
                    let rc = &mut (*params).rgrc[0];
                    if IsZoomed(hwnd) == 0 {
                        let frame_x =
                            GetSystemMetrics(SM_CXSIZEFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                        let frame_y =
                            GetSystemMetrics(SM_CYSIZEFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER);
                        rc.left += frame_x;
                        rc.right -= frame_x;
                        rc.bottom -= frame_y;
                    }
                    // maximized: keep the proposed rect (extends past the
                    // monitor by the frame, so no visible border on screen).
                    return 0;
                }
                WM_NCHITTEST => {
                    // The client covers the top edge, so the default hit test
                    // returns HTCLIENT there and top-edge resize would stop
                    // working. Return HTTOP for the top border band, except
                    // over the header's interactive strip (account chips +
                    // window control buttons, right side).
                    let prev =
                        PREV_WNDPROC.load(std::sync::atomic::Ordering::Relaxed);
                    let prev: extern "system" fn(isize, u32, usize, isize) -> isize =
                        std::mem::transmute(prev);
                    let handled = CallWindowProcW(prev, hwnd, msg, wparam, lparam);
                    if handled == HTCLIENT && IsZoomed(hwnd) == 0 {
                        let (px, py) = hit_test_point(lparam);
                        let (win, frame_y, frame_x) = window_metrics(hwnd);
                        if py >= win.top && py - win.top < frame_y
                            // keep the window-control cluster at the right
                            // edge interactive (close/min/max + chip)
                            && px < win.right - frame_x - 300
                        {
                            return HTTOP;
                        }
                    }
                    handled
                }
                _ => {
                    // NOTE: do NOT read GWLP_WNDPROC here — it holds
                    // `frame_proc` itself after the hook, which would recurse
                    // forever. The original procedure is stashed in
                    // PREV_WNDPROC on install.
                    let prev =
                        PREV_WNDPROC.load(std::sync::atomic::Ordering::Relaxed);
                    let prev: extern "system" fn(isize, u32, usize, isize) -> isize =
                        std::mem::transmute(prev);
                    CallWindowProcW(prev, hwnd, msg, wparam, lparam)
                }
            }
        }
    }

    /// The window procedure that was in place before the hook.
    static PREV_WNDPROC: std::sync::atomic::AtomicIsize =
        std::sync::atomic::AtomicIsize::new(0);

    let mut search = Search {
        pid: std::process::id(),
        found: 0,
    };
    EnumWindows(find_main_window, &mut search as *mut Search as isize);
    let hwnd = search.found;
    if hwnd == 0 {
        return;
    }
    let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
    SetWindowLongPtrW(hwnd, GWL_STYLE, style | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX);
    // hook the window procedure BEFORE asking for a frame recalculation:
    // WM_NCCALCSIZE is what turns the decorated state into "border only".
    // Stash the previous procedure so the hook can forward to it.
    let prev = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
    PREV_WNDPROC.store(prev, std::sync::atomic::Ordering::Relaxed);
    SetWindowLongPtrW(
        hwnd,
        GWLP_WNDPROC,
        frame_proc as extern "system" fn(isize, u32, usize, isize) -> isize as usize as isize,
    );
    // re-apply the frame so the extra border takes effect; the window is
    // kept the same size (the client area still owns the whole surface)
    SetWindowPos(
        hwnd,
        0,
        0,
        0,
        0,
        0,
        SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
    );

    // The window was created hidden (iced Settings.visible = false) so the
    // user never sees the pre-hook state; show it with the final look.
    if IsWindowVisible(hwnd) == 0 {
        ShowWindow(hwnd, SW_SHOW);
    }
}

/// Load a CJK-capable system font so Chinese text renders (iced ships no CJK glyphs).
fn load_cjk_font() -> Option<(Vec<u8>, &'static str)> {
    const CANDIDATES: &[(&str, &str)] = &[
        ("C:\\Windows\\Fonts\\msyh.ttc", "Microsoft YaHei"),
        ("C:\\Windows\\Fonts\\msyh.ttf", "Microsoft YaHei"),
        ("C:\\Windows\\Fonts\\simhei.ttf", "SimHei"),
        ("C:\\Windows\\Fonts\\simsun.ttc", "SimSun"),
        ("C:\\Windows\\Fonts\\Deng.ttf", "DengXian"),
        // Linux / macOS fallbacks
        ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", "Noto Sans CJK SC"),
        ("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc", "WenQuanYi Zen Hei"),
        ("/System/Library/Fonts/PingFang.ttc", "PingFang SC"),
    ];
    for (path, family) in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            return Some((bytes, family));
        }
    }
    None
}

/// Load the bold face of a CJK font family, for M3 emphasis (titles/labels).
fn load_cjk_bold() -> Vec<u8> {
    const BOLD_CANDIDATES: &[&str] = &[
        "C:\\Windows\\Fonts\\msyhbd.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
    ];
    for path in BOLD_CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            return bytes;
        }
    }
    Vec::new()
}

/// Font family actually registered, for building bold variants.
static FONT_FAMILY: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// Bold CJK font for M3 titles / emphasized labels.
fn bold_font() -> iced::Font {
    iced::Font {
        weight: iced::font::Weight::Bold,
        ..iced::Font::new(FONT_FAMILY.get().copied().unwrap_or("Microsoft YaHei"))
    }
}

fn theme(_state: &App) -> Theme {
    styles::theme()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Router,
    Devices,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Router => "路由器插件开关",
            Tab::Devices => "设备加速",
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Tab(Tab),
    IpChanged(String),
    PwdChanged(String),
    MobileChanged(String),
    CodeChanged(String),
    GameFilterChanged(String),

    RouterLogin,
    RouterRefresh,
    RouterSet(bool),
    RouterLogged(String),
    RouterStatus(Option<RouterStatus>),
    RouterErr(String),

    SendCode,
    CloudLogin,
    Logout,
    RefreshAll,
    LoadGames,
    Refreshed(String, Vec<Device>, Option<Status>, Option<(String, String)>),
    CloudSession(String),
    CloudLoginOk(String, String, String),
    AvatarLoaded(Option<iced::widget::image::Handle>),
    /// Animation/ Expiry tick while toasts are visible.
    ToastTick,
    /// Window control buttons / drag zones of the frameless title bar.
    WindowCtl(WinCtl),


    GamesLoaded(GameList),
    OpOk(String),
    AccelOk(String, String, bool),
    OpErr(String),

    SelectDevice(String),
    ToggleDevice(DeviceInfo, bool),
    StartAcc(DeviceInfo),
    StopAcc(DeviceInfo),
    SelectGame(Game),
    SelectZone(Zone),
    SelectServer(Server),

    /// One-shot at startup: give the window the native-frame-with-no-caption
    /// look (resize borders + shadow, custom title bar) and show it.
    FrameBoost,
}

struct App {
    tab: Tab,
    cfg: Config,
    /// native frame boost applied (one-shot Win32 style fix-up)
    frame_boosted: bool,
    ip: String,
    pwd: String,
    mobile: String,
    code: String,
    game_filter: String,

    sid: String,
    router_status: Option<RouterStatus>,

    session: String,
    /// Pre-login /app/* auth session (from SendCode). NOT the login state:
    /// `session` only gains a value after a successful account login.
    auth_sess: String,
    user: String,
    avatar_url: String,
    avatar: Option<iced::widget::image::Handle>,
    router_sess: String,
    devices: Vec<Device>,
    status: Option<Status>,
    games: Vec<Game>,
    gamelist: GameList,
    games_platform: String,

    selected_device: String,
    selected_game: Option<Game>,
    zones: Vec<Zone>,
    servers: Vec<Server>,
    selected_zone: Option<Zone>,
    selected_server: Option<Server>,

    busy: bool,
    /// Active toast stack (newest first); auto-dismissed + slide-in animated.
    toasts: Vec<Toast>,
    /// After a start/stop request: (device key, expected acced state, remaining polls).
    pending_state: Option<(String, bool, u32)>,
}

/// Toast severity → pill coloring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToastKind {
    Info,
    Success,
    Error,
}

/// Frameless-window controls and drag zones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WinCtl {
    Drag,
    Minimize,
    ToggleMax,
    Close,
}

/// One toast notification.
struct Toast {
    text: String,
    kind: ToastKind,
    /// Birth instant, drives slide-in animation and TTL.
    at: std::time::Instant,
    /// Set when the toast starts its exit animation; drives the slide-out.
    closing: Option<std::time::Instant>,
}

/// Total time a toast stays visible.
const TOAST_TTL: std::time::Duration = std::time::Duration::from_millis(3500);
/// Slide-in animation length.
const TOAST_SLIDE: std::time::Duration = std::time::Duration::from_millis(250);
/// Animation tick interval. 33ms keeps the slide-in/out smooth enough while
/// halving the number of full-view updates during a toast's life.
const TOAST_TICK: std::time::Duration = std::time::Duration::from_millis(33);
/// Resting height of a toast pill (13px text + padding + border).
const TOAST_PILL_H: f32 = 30.0;

impl ToastKind {
    fn pill(self, alpha: f32) -> container::Style {
        let a = alpha.clamp(0.0, 1.0);
        let (bg, border, fg) = match self {
            ToastKind::Info => (styles::C::INVERSE_SURFACE, styles::C::INVERSE_SURFACE, styles::C::INVERSE_ON_SURFACE),
            ToastKind::Success => (styles::C::GREEN_CONTAINER, styles::C::GREEN, styles::C::ON_GREEN_CONTAINER),
            ToastKind::Error => (styles::C::ERROR_CONTAINER, styles::C::ERROR, styles::C::ON_ERROR_CONTAINER),
        };
        container::Style {
            snap: false,
            background: Some(Background::Color(styles::faded(bg, a))),
            text_color: Some(styles::faded(fg, a)),
            border: Border {
                color: styles::faded(border, 0.7 * a),
                width: 1.0,
                radius: Radius::from(999.0),
            },
            shadow: Default::default(),
        }
    }
}

impl Default for App {
    fn default() -> Self {
        let cfg = config::load();
        App {
            tab: Tab::Devices,
            ip: cfg.router_ip.clone(),
            pwd: cfg.password(),
            mobile: cfg.mobile.clone(),
            code: String::new(),
            game_filter: String::new(),
            sid: String::new(),
            router_status: None,
            session: cfg.session.clone(),
            auth_sess: String::new(),
            frame_boosted: false,
            user: cfg.user.clone(),
            avatar_url: cfg.avatar_url.clone(),
            avatar: None,
            router_sess: String::new(),
            devices: Vec::new(),
            status: None,
            games: Vec::new(),
            gamelist: GameList::default(),
            games_platform: String::new(),
            selected_device: String::new(),
            selected_game: None,
            zones: Vec::new(),
            servers: Vec::new(),
            selected_zone: None,
            selected_server: None,
            busy: false,
            toasts: Vec::new(),
            pending_state: None,
            cfg,
        }
    }
}

impl App {
    fn persist(&mut self) {
        self.cfg.router_ip = self.ip.clone();
        self.cfg.set_password(&self.pwd);
        self.cfg.mobile = self.mobile.clone();
        self.cfg.session = self.session.clone();
        self.cfg.user = self.user.clone();
        self.cfg.avatar_url = self.avatar_url.clone();
        let _ = config::save(&self.cfg);
    }

    /// Push a new toast (newest first). Returns nothing; expiry is handled
    /// by the toast tick subscription.
    fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.toasts.insert(
            0,
            Toast {
                text: text.into(),
                kind,
                at: std::time::Instant::now(),
                closing: None,
            },
        );
    }

    /// Neutral progress / informational toast.
    fn info(&mut self, text: impl Into<String>) {
        self.toast(text, ToastKind::Info);
    }

    /// Successful operation toast (green pill).
    fn ok(&mut self, text: impl Into<String>) {
        self.toast(text, ToastKind::Success);
    }

    /// Failed operation toast (red pill).
    fn err(&mut self, text: impl Into<String>) {
        self.toast(text, ToastKind::Error);
    }

    fn device_key(info: &DeviceInfo) -> String {
        format!("{}-{}", info.kind, info.mac.to_lowercase())
    }
}

impl std::fmt::Display for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.game_name)
    }
}
impl std::fmt::Display for Zone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.zone_name)
    }
}
impl std::fmt::Display for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.server_name)
    }
}
impl PartialEq for Game {
    fn eq(&self, other: &Self) -> bool {
        self.game_id == other.game_id
    }
}
impl Eq for Game {}
impl PartialEq for Zone {
    fn eq(&self, other: &Self) -> bool {
        self.zone_id == other.zone_id
    }
}
impl Eq for Zone {}
impl PartialEq for Server {
    fn eq(&self, other: &Self) -> bool {
        self.server_id == other.server_id
    }
}
impl Eq for Server {}

/// Run a blocking closure on its own std thread and map the result into a
/// message (iced `Task::blocking` needs no async runtime).
fn task<OUT, T, M>(clo: OUT, map: M) -> Task<Message>
where
    OUT: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
    M: Fn(T) -> Message + Send + 'static,
{
    iced::task::blocking(move |mut sender| {
        let value = clo();
        let _ = sender.try_send(value);
    })
    .map(map)
}

/// Sleep on a std thread, then emit a message.
fn delay<M>(ms: u64, msg: M) -> Task<Message>
where
    M: Fn() -> Message + Send + 'static,
{
    iced::task::blocking(move |mut sender| {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        let _ = sender.try_send(());
    })
    .map(move |_| msg())
}

/// Continuous toast tick stream: fires every TOAST_TICK while the
/// subscription stays subscribed (it's dropped automatically when the
/// toast list goes inactive). Inline so the `Subscription::run`
/// builder can return the inferred stream type directly.


/// Download the account avatar into an iced image handle.
fn avatar_task(url: String) -> Task<Message> {
    if url.is_empty() {
        return Task::none();
    }
    task(
        move || {
            use std::io::Read as _;
            let fail = || None::<iced::widget::image::Handle>;
            let resp = match ureq::get(&url).call() {
                Ok(r) if r.status() < 400 => r,
                _ => return fail(),
            };
            let mut bytes = Vec::new();
            if resp.into_reader().take(1 << 20).read_to_end(&mut bytes).is_err() {
                return fail();
            }
            // Round-crop the avatar (center square + circular alpha mask)
            // so it fits the circular chip without square corners covering
            // the ring border.
            if let Ok(img) = image::load_from_memory(&bytes) {
                use image::imageops::FilterType;
                let rgba = img.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                let side = w.min(h).max(1);
                let cropped = image::imageops::crop_imm(
                    &rgba,
                    (w.saturating_sub(side)) / 2,
                    (h.saturating_sub(side)) / 2,
                    side,
                    side,
                )
                .to_image();
                let scaled = image::imageops::resize(&cropped, 56, 56, FilterType::Lanczos3);
                let (mid, r) = (28.0f32, 28.0f32);
                let mut out = scaled;
                for (x, y, p) in out.enumerate_pixels_mut() {
                    let dx = x as f32 + 0.5 - mid;
                    let dy = y as f32 + 0.5 - mid;
                    if dx * dx + dy * dy > r * r {
                        *p = image::Rgba([0, 0, 0, 0]);
                    }
                }
                let mut png = std::io::Cursor::new(Vec::new());
                image::DynamicImage::ImageRgba8(out)
                    .write_to(&mut png, image::ImageFormat::Png)
                    .ok()?;
                return Some(iced::widget::image::Handle::from_bytes(png.into_inner()));
            }
            Some(iced::widget::image::Handle::from_bytes(bytes.to_vec()))
        },
        Message::AvatarLoaded,
    )
}

/// Refresh again after a short delay (used while waiting for a state change to propagate).
fn delay_refresh(ms: u64) -> Task<Message> {
    delay(ms, || Message::RefreshAll)
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Tab(t) => {
            app.tab = t;
            if t == Tab::Devices && !app.session.is_empty() {
                Task::done(Message::RefreshAll)
            } else {
                Task::none()
            }
        }
        Message::IpChanged(v) => {
            app.ip = v;
            Task::none()
        }
        Message::PwdChanged(v) => {
            app.pwd = v;
            Task::none()
        }
        Message::MobileChanged(v) => {
            app.mobile = v;
            Task::none()
        }
        Message::CodeChanged(v) => {
            app.code = v;
            Task::none()
        }
        Message::GameFilterChanged(v) => {
            app.game_filter = v;
            Task::none()
        }

        Message::RouterLogin => {
            app.busy = true;
            app.info("登录路由器…");
            app.persist();
            let ip = app.ip.clone();
            let pwd = app.pwd.clone();
            task(move || {
 router::login(&ip, &pwd) },
                |r| match r {
                    Ok(sid) => Message::RouterLogged(sid),
                    Err(e) => Message::RouterErr(e),
                },
            )
        }
        Message::RouterRefresh => {
            if app.sid.is_empty() {
                app.err("请先登录路由器");
                return Task::none();
            }
            let ip = app.ip.clone();
            let sid = app.sid.clone();
            app.busy = true;
            task(move || {
 router::get_status(&ip, &sid) },
                |r| Message::RouterStatus(r.ok()),
            )
        }
        Message::RouterSet(on) => {
            if app.sid.is_empty() {
                app.err("请先登录路由器");
                return Task::none();
            }
            let ip = app.ip.clone();
            let sid = app.sid.clone();
            app.busy = true;
            task(move || {

                    router::set_enable(&ip, &sid, on)?;
                    router::get_status(&ip, &sid)
                },
                |r| match r {
                    Ok(st) => Message::RouterStatus(Some(st)),
                    Err(e) => Message::RouterErr(e),
                },
            )
        }
        Message::RouterLogged(sid) => {
            app.sid = sid;
            app.ok("路由器登录成功");
            let ip = app.ip.clone();
            let sid = app.sid.clone();
            task(move || {
 router::get_status(&ip, &sid) },
                |r| Message::RouterStatus(r.ok()),
            )
        }
        Message::RouterStatus(st) => {
            app.busy = false;
            if let Some(s) = st {
                app.info(format!("路由器状态: {}", s.summary()));
                app.router_status = Some(s);
            }
            Task::none()
        }
        Message::RouterErr(e) => {
            app.busy = false;
            app.err(format!("路由器错误: {}", e));
            Task::none()
        }

        Message::SendCode => {
            let mobile = app.mobile.clone();
            let sess = if app.auth_sess.is_empty() {
                String::new()
            } else {
                app.auth_sess.clone()
            };
            app.busy = true;
            app.info("请求验证码…");
            task(move || {

                    let sess = if sess.is_empty() { auth()? } else { sess };
                    let body = send_code(&sess, &mobile)?;
                    Ok::<(String, String), String>((sess, body))
                },
                |r| match r {
                    Ok((sess, body)) => Message::CloudSession(format!("{}|{}", sess, body)),
                    Err(e) => Message::OpErr(e),
                },
            )
        }
        Message::CloudLogin => {
            let mobile = app.mobile.clone();
            let code = app.code.clone();
            let sess = app.auth_sess.clone();
            app.busy = true;
            app.info("登录中…");
            task(move || {

                    let sess = if sess.is_empty() { auth()? } else { sess };
                    let v = login(&sess, &mobile, &code)?;
                    let ui = v.get("user_info");
                    let name = ui
                        .and_then(|u| u.get("nickname"))
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| mobile.clone());
                    let avatar = ui
                        .and_then(|u| u.get("avatar").or_else(|| u.get("face_url")))
                        .and_then(|a| a.as_str())
                        .unwrap_or("")
                        .to_string();
                    Ok::<(String, String, String), String>((sess, name, avatar))
                },
                |r| match r {
                    Ok((sess, name, avatar)) => Message::CloudLoginOk(sess, name, avatar),
                    Err(e) => Message::OpErr(e),
                },
            )
        }
        Message::CloudSession(s) => {
            let mut parts = s.splitn(2, '|');
            // this is only the PRE-login auth session: the app is NOT logged
            // in until CloudLoginOk — do not touch `session` here
            if let Some(sess) = parts.next() {
                app.auth_sess = sess.to_string();
            }
            if let Some(body) = parts.next() {
                app.info(format!("验证码: {}", body));
            }
            app.busy = false;
            Task::none()
        }
        Message::CloudLoginOk(sess, name, avatar) => {
            app.session = sess;
            app.auth_sess.clear(); // consumed by the successful login
            app.code.clear();
            if !name.is_empty() {
                app.user = name;
            }
            if !avatar.is_empty() {
                app.avatar_url = avatar;
            }
            app.persist();
            app.busy = false;
            app.ok(format!("登录成功: {}", app.user));
            Task::done(Message::RefreshAll)
        }
        Message::AvatarLoaded(h) => {
            app.avatar = h;
            Task::none()
        }
        Message::ToastTick => {
            let now = std::time::Instant::now();
            // Start the exit animation once TTL is reached; drop toasts
            // whose exit animation has finished.
            for t in app.toasts.iter_mut() {
                if t.closing.is_none() && now.duration_since(t.at) >= TOAST_TTL {
                    t.closing = Some(now);
                }
            }
            app.toasts.retain(|t| match t.closing {
                Some(d) => now.duration_since(d) < TOAST_SLIDE,
                None => true,
            });
            // The tick subscription stops once a toast finishes entering.
            // Schedule a one-shot wake-up for each static toast so the exit
            // still starts on time when its TTL lapses.
            let mut wake = Vec::new();
            for t in &app.toasts {
                if t.closing.is_none() {
                    let elapsed = now.duration_since(t.at);
                    if elapsed >= TOAST_SLIDE && elapsed < TOAST_TTL {
                        let wait = TOAST_TTL - elapsed;
                        wake.push(task(
                            move || {
                                std::thread::sleep(wait);
                            },
                            |_| Message::ToastTick,
                        ));
                    }
                }
            }
            Task::batch(wake)
        }
        Message::WindowCtl(w) => iced::window::latest().then(move |id| match id {
            Some(id) => match w {
                WinCtl::Drag => iced::window::drag::<Message>(id),
                WinCtl::Minimize => iced::window::minimize::<Message>(id, true),
                WinCtl::ToggleMax => {
                    let t = iced::window::toggle_maximize::<Message>(id);
                    Task::none().chain(t)
                }
                WinCtl::Close => iced::window::close::<Message>(id),
            },
            None => Task::none(),
        }),
        Message::FrameBoost => {
            if !app.frame_boosted {
                app.frame_boosted = true;
                // The window was created decorated-but-invisible: hook the
                // procedure to strip the caption (custom title bar); the
                // first ShowWindow inside does the initial reveal.
                #[cfg(windows)]
                unsafe {
                    boost_window_frame();
                }
                // Then re-assert visibility THROUGH winit so its internal
                // VISIBLE flag becomes `true`: otherwise the next window
                // diff (maximize/minimize) hits `!VISIBLE → SW_HIDE` and
                // makes the window disappear ("flash exit").
                iced::window::latest().then(|id| match id {
                    Some(id) => iced::window::set_visible(id, true),
                    None => Task::none(),
                })
            } else {
                Task::none()
            }
        }
        Message::Logout => {
            app.session.clear();
            app.auth_sess.clear();
            app.code.clear();
            app.user.clear();
            app.devices.clear();
            app.status = None;
            app.pending_state = None;
            app.persist();
            app.info("已退出登录");
            Task::none()
        }
        Message::RefreshAll => {
            let session = app.session.clone();
            let sn = app.cfg.sn.clone();
            if session.is_empty() {
                app.err("请先登录 UU 账号");
                return Task::none();
            }
            app.busy = true;
            task(move || {

                    let rs = get_router_sess(&sn).unwrap_or_default();
                    let ui = userinfo(&session).ok();
                    let _ = bind_router(&session, &rs);
                    let devs = get_devices(&session, Some(&rs))?;
                    let st = get_status(&session, &rs).ok();
                    let mut v = Vec::new();
                    for g in devs.all_devices {
                        v.extend(g.devices);
                    }
                    Ok::<(String, Vec<Device>, Option<Status>, Option<(String, String)>), String>((
                        rs, v, st, ui,
                    ))
                },
                |r| match r {
                    Ok((rs, devs, st, ui)) => Message::Refreshed(rs, devs, st, ui),
                    Err(e) => Message::OpErr(e),
                },
            )
        }
        Message::LoadGames => {
            let session = app.session.clone();
            let platform = platform_for(&app.selected_device);
            app.games_platform = platform.clone();
            app.busy = true;
            task(move || {
 gamelist(&session, &platform) },
                |r| match r {
                    Ok(gl) => Message::GamesLoaded(gl),
                    Err(e) => Message::OpErr(e),
                },
            )
        }
        Message::Refreshed(rs, devs, st, ui) => {
            app.router_sess = rs;
            app.devices = devs;
            if st.is_some() {
                app.status = st;
            }
            app.busy = false;
            let mut t = Task::none();
            if let Some((name, face)) = ui {
                let mut changed = false;
                if !name.is_empty() && app.user != name {
                    app.user = name;
                    changed = true;
                }
                if !face.is_empty() && app.avatar_url != face {
                    app.avatar_url = face;
                    app.avatar = None;
                    changed = true;
                }
                if changed {
                    app.persist();
                }
                if app.avatar.is_none() && !app.avatar_url.is_empty() {
                    t = avatar_task(app.avatar_url.clone());
                }
            }
            if let Some((key, expected, left)) = app.pending_state.clone() {
                let updated = app.devices.iter().any(|d| {
                    App::device_key(&d.deviceinfo) == key && d.deviceinfo.acced == expected
                });
                if updated {
                    app.pending_state = None;
                    app.ok("设备状态已更新");
                } else if left > 0 {
                    app.pending_state = Some((key, expected, left - 1));
                    t = Task::batch([t, delay_refresh(1500)]);
                } else {
                    app.pending_state = None;
                    app.err("状态更新超时，请稍后手动刷新");
                }
            } else {
                app.ok("设备列表刷新完成");
            }
            t
        }
        Message::GamesLoaded(gl) => {
            app.games = gl.flat();
            app.gamelist = gl;
            app.busy = false;
            app.info(format!("加载 {} 个游戏", app.games.len()));
            Task::none()
        }
        Message::OpOk(msg) => {
            app.busy = false;
            app.ok(msg);
            if app.session.is_empty() {
                Task::none()
            } else {
                Task::done(Message::RefreshAll)
            }
        }
        Message::AccelOk(msg, key, expected) => {
            app.busy = false;
            app.ok(msg);
            app.pending_state = Some((key, expected, 10));
            // Cloud accepted the request: go back to the device list.
            app.selected_device.clear();
            app.selected_game = None;
            app.zones.clear();
            app.servers.clear();
            app.selected_zone = None;
            app.selected_server = None;
            Task::done(Message::RefreshAll)
        }
        Message::OpErr(e) => {
            app.busy = false;
            app.err(format!("错误: {}", e));
            Task::none()
        }
        Message::StartAcc(info) => {
            let session = app.session.clone();
            let rs = app.router_sess.clone();
            let key = app.selected_device.clone();
            let game = app.selected_game.clone();
            let zone = app.selected_zone.clone();
            let server = app.selected_server.clone();
            app.busy = true;
            app.info("正在开始加速…");
            task(move || {

                    if let Some(g) = &game {
                        set_game(&session, &rs, &key, g, zone.as_ref(), server.as_ref())?;
                    }
                    start_acc(&session, &rs, &info)?;
                    Ok::<(String, String, bool), String>(("已开始加速".into(), key, true))
                },
                |r| match r {
                    Ok((m, k, e)) => Message::AccelOk(m, k, e),
                    Err(e) => Message::OpErr(e),
                },
            )
        }
        Message::StopAcc(info) => {
            let session = app.session.clone();
            let rs = app.router_sess.clone();
            let key = app.selected_device.clone();
            app.busy = true;
            app.info("正在停止加速…");
            task(move || {

                    stop_acc(&session, &rs, &info)?;
                    Ok::<(String, String, bool), String>(("已停止加速".into(), key, false))
                },
                |r| match r {
                    Ok((m, k, e)) => Message::AccelOk(m, k, e),
                    Err(e) => Message::OpErr(e),
                },
            )
        }
        Message::SelectDevice(key) => {
            if key.is_empty() || key == app.selected_device {
                app.selected_device.clear();
                app.selected_game = None;
                app.game_filter.clear();
                app.zones.clear();
                app.servers.clear();
                app.selected_zone = None;
                app.selected_server = None;
                return Task::none();
            }
            app.selected_device = key.clone();
            app.selected_game = None;
            app.game_filter.clear();
            app.zones.clear();
            app.servers.clear();
            app.selected_zone = None;
            app.selected_server = None;
            // Drop the previous device's game list immediately: it belongs to
            // another device (or is stale) and showing it while the fresh
            // list loads is misleading. The accel view shows a 「正在加载」
            // placeholder instead.
            app.games.clear();
            let platform = platform_for(&key);
            app.games_platform = platform.clone();
            Task::done(Message::LoadGames)
        }
        Message::ToggleDevice(info, on) => {
            let session = app.session.clone();
            let rs = app.router_sess.clone();
            let label: &'static str = if on { "已开启" } else { "已关闭" };
            app.busy = true;
            task(
                move || auto_acc(&session, &rs, &info, on),
                move |r| match r {
                    Ok(_) => Message::OpOk(label.into()),
                    Err(e) => Message::OpErr(e),
                },
            )
        }
        Message::SelectGame(g) => {
            // clicking the already-selected game toggles the selection off
            if app.selected_game.as_ref() == Some(&g) {
                app.selected_game = None;
                app.zones.clear();
                app.servers.clear();
                app.selected_zone = None;
                app.selected_server = None;
                return Task::none();
            }
            app.selected_game = Some(g.clone());
            let zones = app.gamelist.zonelist.get(&g.game_id).cloned().unwrap_or_default();
            app.selected_zone = zones.first().cloned();
            app.zones = zones;
            let servers = app
                .selected_zone
                .as_ref()
                .and_then(|z| app.gamelist.serverlist.get(&z.zone_id).cloned())
                .unwrap_or_default();
            app.selected_server = servers.first().cloned();
            app.servers = servers;
            Task::none()
        }
        Message::SelectZone(z) => {
            let servers = app.gamelist.serverlist.get(&z.zone_id).cloned().unwrap_or_default();
            app.selected_server = servers.first().cloned();
            app.servers = servers;
            app.selected_zone = Some(z);
            Task::none()
        }
        Message::SelectServer(s) => {
            app.selected_server = Some(s);
            Task::none()
        }
    }
}

fn platform_for(device_key: &str) -> String {
    let kind = device_key.split('-').next().unwrap_or("");
    match kind {
        "windows" | "macos" | "pc" => "pc".into(),
        "android" | "apple" | "ios" => "mobile".into(),
        other => other.into(),
    }
}

// ---------------- view ----------------

fn view(app: &App) -> impl Widget<Message> + '_ {
    let account = if !app.user.is_empty() {
        format!("已登录: {}", app.user)
    } else if !app.session.is_empty() {
        "已登录".to_string()
    } else {
        "未登录".to_string()
    };
    // Explorer-style tab strip on top of the main card: the active tab
    // shares the card's surface (visually fused with the content below),
    // inactive tabs are recessed darker chips.
    let mut tabs_row = iced::widget::row![].spacing(2).align_y(iced::Alignment::End);
    for tab in [Tab::Devices, Tab::Router] {
        tabs_row = tabs_row.push(tab_button(app, tab));
    }
    let tabs = container(tabs_row)
        .width(Length::Shrink)
        .height(Length::Fill)
        .align_y(iced::Alignment::End)
        .padding(iced::Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 12.0,
        });
    let mut header = row![tabs,
        // drag zone: the whole empty strip above the content is draggable
        // (3/4 of the free width: keeps the account near the right side;
        // the remaining 1/4 goes to the win-control area which centers
        // the cluster between the account and the window right edge)
        mouse_area(iced::widget::Space::new()
            .width(Length::FillPortion(3))
            .height(Length::Fill))
            .on_press(Message::WindowCtl(WinCtl::Drag)),
    ]
    // spacing 0: the gap to the account is inside its wrapper (left pad);
    // this keeps the win-control cluster EXACTLY centered between the
    // account block and the window right edge
    .spacing(0)
    .width(Length::Fill)
    .height(Length::Fill);
    // account block: hugging the right side, just left of the win controls
    // (boxed children: avatar / label / logout have different widget types)
    let mut account_row: Vec<iced::Element<'_, Message>> = Vec::new();
    if let Some(h) = &app.avatar {
        account_row.push(
            container(iced::widget::image(h.clone()).width(28).height(28))
                .width(30)
                .height(30)
                .clip(true)
                .align_x(iced::Alignment::Center)
                .align_y(iced::Alignment::Center)
                .style(styles::avatar)
                .boxed(),
        );
    }
    account_row.push(
        text(account)
            .size(13)
            .color(styles::C::ON_SURFACE_VARIANT)
            .boxed(),
    );
    if !app.session.is_empty() {
        account_row.push(
            icon_button('\u{E8AB}', Message::Logout, styles::icon_outlined, 28.0).boxed(),
        );
    }
    header = header.push(
        container(row(account_row).spacing(10).align_y(iced::Alignment::Center))
            .width(Length::Shrink)
            .height(Length::Fill)
            .align_y(iced::Alignment::Center)
            .padding(iced::Padding {
                // left: gap to the drag zone (row spacing is 0); right pad
                // must stay 0 so the centered cluster keeps equal gaps
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 10.0,
            })
            .boxed(),
    );
    // Window controls: a compact fixed cluster pinned to the right edge.
    // Fixed width so the button spacing stays tight at any window size —
    // the previous FillPortion-based stretch spread the buttons across a
    // quarter of the screen when maximized.
    let right_area = container(
        row![
            win_button('\u{E921}', Message::WindowCtl(WinCtl::Minimize), false),
            win_button('\u{E922}', Message::WindowCtl(WinCtl::ToggleMax), false),
            win_button('\u{E8BB}', Message::WindowCtl(WinCtl::Close), true),
        ]
        .spacing(2)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fixed(92.0))
    .height(Length::Fill)
    .align_x(iced::Alignment::Center)
    .align_y(iced::Alignment::Center)
    .boxed();
    header = header.push(right_area);
    // The strip is full-bleed to the window's top/right edges: the tabs hang
    // from its bottom edge (fusing with the card), the window controls span
    // its full height and touch the top border — chrome layout.
    // NOTE: this strip now paints the window's base SURFACE itself; the
    // root no longer fills the whole window (the card covers it all).
    let header = container(header)
        .style(styles::window)
        .height(Length::Fixed(42.0))
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 0.0,
            right: 1.0,
            bottom: 0.0,
            left: 1.0,
        });

    let content = match app.tab {
        Tab::Router => view_router(app).boxed(),
        Tab::Devices => view_devices(app).boxed(),
    };
    let content = container(content)
        .padding(iced::Padding {
            top: 14.0,
            right: 16.0,
            bottom: 16.0,
            left: 18.0,
        })
        .width(Length::Fill)
        .height(Length::Fill);

    // The card holds ONLY the content; the tab strip floats on the window
    // background above it. The active tab is painted with the card's color
    // and touches the card's top edge — the connected-tab look. The window
    // padding wraps the card only: the strip is full-bleed to the edges.
    let card_surface = container(content)
        .style(styles::card)
        .width(Length::Fill)
        .height(Length::Fill);
    let main_card = container(card_surface)
        .padding(iced::Padding {
            top: 0.0,
            // no rim: the card fills the window below the strip
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        })
        .width(Length::Fill)
        .height(Length::Fill);
    let window_col = column![header, main_card]
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill);

    // Frameless window root with a subtle primary border for definition.
    let window_root = container(window_col)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(styles::window);

    let now = std::time::Instant::now();
    let slide_s = TOAST_SLIDE.as_secs_f32();
    // ease-out cubic
    let ease = |p: f32| 1.0 - (1.0 - p).powi(3);

    // Toast pill, newest on top, top-center anchored; every pill is
    // horizontally centered within the stack regardless of its width.
    let mut toasts = column![].spacing(8).align_x(iced::Alignment::Center);
    for t in app.toasts.iter() {
        // Mask reveal fraction (1 = slot fully open) and pill opacity.
        // The mask moves (height) ONLY while the pill is fully
        // transparent — iced propagates height constraints down the
        // tree, so exposing the pill while the slot is small would
        // collapse its text layout (zero-height lines, narrow pill).
        let (h_frac, alpha) = match t.closing {
            None => {
                let pr = (now.duration_since(t.at).as_secs_f32() / slide_s)
                    .clamp(0.0, 1.0);
                let mask = ease((pr / 0.4).clamp(0.0, 1.0)); // slot opens in 40%
                let fade = ((pr - 0.4) / 0.6).clamp(0.0, 1.0); // then fades in
                (mask, ease(fade))
            }
            Some(d) => {
                let q = (now.duration_since(d).as_secs_f32() / slide_s).clamp(0.0, 1.0);
                if q < 0.6 {
                    // fade out at full height
                    (1.0, 1.0 - (q / 0.6))
                } else {
                    // slot closes while invisible
                    (1.0 - ((q - 0.6) / 0.4), 0.0)
                }
            }
        };
        let kind = t.kind;
        // Pin the pill's height so its layout is stable while the mask
        // animates around it.
        let pill = container(text(fit(&t.text, 95)).size(13))
            .padding([5, 14])
            .height(Length::Fixed(TOAST_PILL_H))
            .style(move |_| kind.pill(alpha));
        let h = TOAST_PILL_H * h_frac;
        if h < TOAST_PILL_H - 0.5 {
            toasts = toasts.push(
                container(pill)
                    .height(Length::Fixed(h))
                    .clip(true)
                    .align_x(iced::Alignment::Center)
                    .align_y(iced::Alignment::Center)
                    .boxed(),
            );
        } else {
            toasts = toasts.push(pill.boxed());
        }
    }

    let overlay = container(toasts)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(iced::Padding {
            top: 20.0,
            right: 20.0,
            left: 20.0,
            bottom: 0.0,
        })
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Start)
        .boxed();

    // Toast overlay is only composited when a toast exists. An always-on
    // `stack!` would re-run the full widget-tree `mouse_interaction` walk
    // on EVERY draw (Stack::layers_under), which is O(children) per frame
    // — the 300-row game list made every scroll frame pay that walk.
    let root: iced::Element<'_, Message> = if app.toasts.is_empty() {
        window_root.boxed()
    } else {
        iced::widget::stack![window_root, overlay].boxed()
    };
    root
}

/// Circular icon-only button (`size` = diameter in logical px).
fn icon_button<'a>(
    glyph: char,
    msg: Message,
    style: fn(&Theme, button::Status) -> button::Style,
    size: f32,
) -> impl Widget<Message> + 'a {
    button(
        container(
            text(glyph.to_string())
                .size(14)
                .font(MDL2_FONT),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .padding(0) // kill Button default padding{5,10}
    .on_press(msg)
    .style(style)
}

/// Frameless-window control button (minimize / maximize / close):
/// small circular hover chip, centered in the strip.
fn win_button<'a>(
    glyph: char,
    msg: Message,
    danger: bool,
) -> impl Widget<Message> + 'a {
    button(
        container(
            text(glyph.to_string())
                .size(11)
                .font(MDL2_FONT)
                .color(styles::C::ON_SURFACE_VARIANT),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fixed(28.0))
    .height(Length::Fixed(28.0))
    .padding(0) // kill Button default padding{5,10}
    .on_press(msg)
    .style(if danger {
        styles::win_close
    } else {
        styles::win_ctl
    })
}

fn tab_button<'a>(
    app: &'a App,
    tab: Tab,
) -> impl Widget<Message> + 'a {
    let active = app.tab == tab;
    button(text(tab.label()).size(14))
        .padding([7, 18])
        .on_press(Message::Tab(tab))
        .style(if active {
            styles::tab_active
        } else {
            styles::tab_inactive
        })
}

fn view_router(app: &App) -> impl Widget<Message> + '_ {
    let login = row![
        text_input("路由器地址", &app.ip)
            .on_input(Message::IpChanged)
            .style(styles::text_field)
            .padding([7, 10])
            .width(Length::Fixed(200.0)),
        text_input("管理密码", &app.pwd)
            .on_input(Message::PwdChanged)
            .secure(true)
            .style(styles::text_field)
            .padding([7, 10])
            .width(Length::Fixed(200.0)),
        button("登录").on_press(Message::RouterLogin).padding([7, 20]).style(styles::filled),
        button("刷新").on_press(Message::RouterRefresh).padding([7, 20]).style(styles::tonal),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let status_text = match &app.router_status {
        Some(s) => format!("{}    （UU 开关: {}）", s.summary(), if s.enabled() { "开" } else { "关" }),
        None => "未登录或未获取状态".into(),
    };

    let controls = row![
        button("开启 UU 插件").on_press(Message::RouterSet(true)).padding([8, 20]).style(styles::success_filled),
        button("关闭 UU 插件").on_press(Message::RouterSet(false)).padding([8, 20]).style(styles::error_tonal),
    ]
    .spacing(8);

    column![
        text("路由器登录").size(16).font(bold_font()),
        login,
        text("目前仅支持锐捷X60Pro").size(13).color(styles::C::ON_SURFACE_VARIANT),
        space::vertical().height(6),
// thin divider between sections (single-pixel filled line)
container(space::vertical().height(1.0))
    .height(Length::Fixed(1.0))
    .width(Length::Fill)
    .style(styles::divider),
        space::vertical().height(6),
        text("状态").size(16).font(bold_font()),
        column![
            text(status_text).color(styles::C::ON_SURFACE_VARIANT),
            controls,
        ]
        .spacing(10),
    ]
    .spacing(10)
}

const W_NAME: f32 = 100.0;
const W_HOST: f32 = 170.0;
const W_IP: f32 = 150.0;
const W_MAC: f32 = 170.0;
const W_STATE: f32 = 74.0;
/// Display budget (columns; CJK counts as 2) of a card's game line.
const GAME_BUDGET: usize = 70;

/// Truncate to a display budget (CJK counts as 2 columns) and append an ellipsis.
fn fit(s: &str, budget: usize) -> String {
    if s.trim().is_empty() {
        return "—".into();
    }
    let mut used = 0usize;
    let mut out = String::new();
    for ch in s.chars() {
        let w = if (ch as u32) > 0x2E80 { 2 } else { 1 };
        if used + w > budget {
            out.push('…');
            break;
        }
        used += w;
        out.push(ch);
    }
    out
}

/// Column header cell (the sticky-looking header row of the device list).
fn cell_head<'a>(s: &'a str, w: f32) -> impl Widget<Message> + 'a {
    let budget = (w / 9.0) as usize;
    container(
        text(fit(s, budget)).size(14).color(styles::C::ON_SURFACE_VARIANT)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fixed(w))
    .height(Length::Fixed(26.0))
}

/// Font Awesome brand icon (font, glyph) for a device kind.
fn type_icon(kind: &str) -> (iced::Font, char) {
    // Steam Deck runs Linux but is a handheld — matched before "linux".
    let k = kind.to_lowercase();
    if k.contains("steam") || k.contains("deck") {
        return (FA_BRANDS_FONT, '\u{f1b6}'); // steam
    }
    if k.contains("playstation") || k.contains("ps4") || k.contains("ps5") || k.contains("psv") {
        return (FA_BRANDS_FONT, '\u{f3df}'); // playstation
    }
    if k.contains("xbox") {
        return (FA_BRANDS_FONT, '\u{f412}'); // xbox
    }
    if k.contains("linux") {
        return (FA_BRANDS_FONT, '\u{f17c}'); // linux
    }
    match k.as_str() {
        "android" => (FA_BRANDS_FONT, '\u{f17b}'),      // android robot
        "apple" | "ios" | "macos" | "mac" => (FA_BRANDS_FONT, '\u{f179}'), // apple
        "windows" | "pc" | "win" => (FA_BRANDS_FONT, '\u{f17a}'), // windows logo
        _ => (MDL2_FONT, '\u{E7EC}'),                    // gamepad (console / other)
    }
}

/// True when a device reports a real hostname (non-empty and not merely
/// its own IP address echoed back by the cloud).
fn has_real_name(hostname: &str) -> bool {
    let h = hostname.trim();
    !h.is_empty() && h.parse::<std::net::Ipv4Addr>().is_err()
}

/// Device category ranking for list ordering: consoles → handhelds →
/// PC (Windows / macOS / Linux) → mobile → anything else.
/// Steam Deck counts as a handheld even though it runs Linux.
fn category_rank(kind: &str) -> u8 {
    let k = kind.to_lowercase();
    if k.contains("steam") || k.contains("deck") || k.contains("switch") {
        return 1; // handheld
    }
    if k.contains("playstation")
        || k.contains("ps4")
        || k.contains("ps5")
        || k.contains("psv")
        || k.contains("xbox")
    {
        return 0; // home console
    }
    if k.contains("linux") || matches!(k.as_str(), "windows" | "pc" | "win" | "macos" | "mac") {
        return 2; // PC
    }
    if matches!(k.as_str(), "android" | "apple" | "ios" | "mobile" | "phone" | "tablet")
        || k.contains(" ios")
    {
        return 3; // mobile
    }
    4 // unknown kinds last
}

fn device_card<'a>(app: &'a App, d: &'a Device) -> impl Widget<Message> + 'a {
    let info = &d.deviceinfo;
    let key = App::device_key(info);
    let selected = app.selected_device == key;
    let state = if info.acced { "加速中" } else { "未加速" };
    let game = app
        .status
        .as_ref()
        .and_then(|s| s.game_mode_setting.device_setting.get(&key))
        .map(|m| {
            if m.is_valid() {
                format!("{} / {} / {}", m.game_name, m.zone_name, m.server_name)
            } else {
                "未设置或无效".to_string()
            }
        })
        .unwrap_or_else(|| "未设置游戏".to_string());

    // Lean card: every run is truncated by `fit` to its column budget, so no
    // per-cell clip layers or wrapper containers are needed (tiny-skia draws
    // the whole window each scroll frame; fewer draws = smoother scrolling).
    let (font, glyph) = type_icon(&info.kind);
    let state_color = if info.acced {
        styles::C::GREEN
    } else {
        styles::C::ON_SURFACE_VARIANT
    };

    let head = row![
        text(fit(&info.hostname, 19))
            .size(16)
            .font(bold_font())
            .wrapping(iced::widget::text::Wrapping::None)
            .width(Length::Fixed(W_HOST)),
        text(format!("{} {}", glyph, fit(&d.name, 8)))
            .size(14)
            .font(font)
            .wrapping(iced::widget::text::Wrapping::None)
            .width(Length::Fixed(W_NAME)),
        text(&info.ip)
            .size(16)
            .wrapping(iced::widget::text::Wrapping::None)
            .width(Length::Fixed(W_IP)),
        text(&info.mac)
            .size(16)
            .wrapping(iced::widget::text::Wrapping::None)
            .width(Length::Fixed(W_MAC)),
        text(format!("● {}", state))
            .size(15)
            .color(state_color)
            .wrapping(iced::widget::text::Wrapping::None)
            .width(Length::Fixed(W_STATE)),
    ]
    .spacing(10)
    .width(Length::Fill)
    .align_y(iced::Alignment::Center);

    let sub = text(fit(&game, GAME_BUDGET))
        .size(13)
        .color(styles::C::ON_SURFACE_VARIANT)
        .wrapping(iced::widget::text::Wrapping::None);

    button(column![head, sub].spacing(6).width(Length::Fill))
        .on_press(Message::SelectDevice(key))
        .padding(14)
        .width(Length::Fill)
        .style(move |theme, status| {
            if selected {
                styles::device_card_selected(theme, status)
            } else if d.deviceinfo.acced {
                styles::device_card_acced(theme, status)
            } else {
                styles::device_card(theme, status)
            }
        })
}

fn view_devices(app: &App) -> impl Widget<Message> + '_ {
    // uniform 32px control height: text inputs are padding(5)+line(18) tall by
    // default while padded buttons came out taller — pad both to 18+14=32
    let login = row![
        text_input("手机号", &app.mobile)
            .on_input(Message::MobileChanged)
            .style(styles::text_field)
            .padding([7, 10])
            .width(Length::Fixed(160.0)),
        button("发送验证码")
            .on_press(Message::SendCode)
            .padding([7, 16])
            .style(styles::tonal),
        text_input("短信验证码", &app.code)
            .on_input(Message::CodeChanged)
            .style(styles::text_field)
            .padding([7, 10])
            .width(Length::Fixed(120.0)),
        button("登录")
            .on_press(Message::CloudLogin)
            .padding([7, 24])
            .style(styles::filled),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let header = container(
        row![
            cell_head("设备", W_HOST),
            cell_head("类型", W_NAME),
            cell_head("IP 地址", W_IP),
            cell_head("MAC", W_MAC),
            cell_head("状态", W_STATE),
        ]
        .spacing(10)
        .width(Length::Fill)
        .align_y(iced::Alignment::Center),
    )
    .padding([6, 12]);

    let mut list = column![].spacing(8);
    if app.devices.is_empty() {
        list = list.push(
            container(
                text("暂无设备，请先登录后刷新").size(14)
                    .color(styles::C::ON_SURFACE_VARIANT),
            )
                .padding(24)
                .width(Length::Fill)
                .center_x(Length::Fill)
                .boxed(),
        );
    }
    let mut ordered: Vec<&Device> = app.devices.iter().collect();
    // Accelerating devices pinned on top; then by category
    // (console → handheld → PC → mobile → other); devices with a real
    // hostname (not just the IP echoed back) before anonymous ones;
    // stable inside groups.
    ordered.sort_by_key(|d| {
        let info = &d.deviceinfo;
        (!info.acced, category_rank(&info.kind), !has_real_name(&info.hostname))
    });
    for d in ordered {
        list = list.push(device_card(app, d).boxed());
    }

    let list_head = row![
        text("设备列表").size(16).font(bold_font()),
        space::horizontal(),
        icon_button('\u{E72C}', Message::RefreshAll, styles::icon_tonal, 34.0),
    ]
    .width(Length::Fill)
    .align_y(iced::Alignment::Center)
    .boxed();

    // boxed children: unifies the heterogeneous branches below
    let mut col: Vec<iced::Element<'_, Message>> = Vec::new();
    if app.session.is_empty() {
        col.push(
            column![
                text("UU 账号登录").size(16).font(bold_font()),
                login,
            ]
            .spacing(10)
            .boxed(),
        );
    }
    if app.selected_device.is_empty() {
        col.push(list_head);
        // the header row is INSIDE the scrollable: it scrolls with the list
        col.push(
            container(
                scrollable(column![header, list].spacing(6).width(Length::Fill))
                    .spacing(10)
                    .style(styles::scroll_bar)
                    .height(Length::Fill),
            )
            .padding(iced::Padding {
                right: 0.0,
                left: 0.0,
                top: 0.0,
                bottom: 2.0,
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .boxed(),
        );
    } else {
        col.push(view_accel(app).boxed());
    }
    column(col).spacing(14).height(Length::Fill)
}

fn view_accel(app: &App) -> impl Widget<Message> + '_ {
    let key = app.selected_device.clone();
    let found = app
        .devices
        .iter()
        .find(|d| App::device_key(&d.deviceinfo) == key);
    let (title, info, acced, auto_on) = match found {
        Some(d) => {
            let label = if d.deviceinfo.hostname.trim().is_empty() {
                d.name.clone()
            } else {
                d.deviceinfo.hostname.clone()
            };
            (
                format!("{}  ({})", label, d.deviceinfo.ip),
                d.deviceinfo.clone(),
                d.deviceinfo.acced,
                d.custom
                    .get("auto_acc")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            )
        }
        None => (key.clone(), DeviceInfo::default(), false, false),
    };

    let head = row![
        button("← 返回").on_press(Message::SelectDevice(String::new())).padding([6, 14]).style(styles::text_btn),
        text(fit(
            &format!(
                "为 {} 加速 · {}",
                title,
                if acced { "状态: 加速中" } else { "状态: 未加速" }
            ),
            70,
        ))
        .size(16)
        .font(bold_font()),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center)
    .boxed();

    // last accelerated game for this device (what the device list shows)
    let last_game = app
        .status
        .as_ref()
        .and_then(|s| s.game_mode_setting.device_setting.get(&key))
        .filter(|m| m.is_valid());

    let controls = row![
        button(if auto_on {
            "关闭自动加速"
        } else {
            "开启自动加速"
        })
        .on_press(Message::ToggleDevice(info.clone(), !auto_on))
        .padding([8, 16])
        .style(if auto_on {
            styles::error_tonal
        } else {
            styles::tonal
        }),
        // an accelerating device shows 停止加速 only while no (new) game is
        // selected; picking a game switches it to 开始加速 for that game
        if acced && app.selected_game.is_none() {
            button("停止加速")
                .on_press(Message::StopAcc(info.clone()))
                .padding([8, 24])
                .style(styles::error_tonal)
        } else {
            button("开始加速")
                .on_press(Message::StartAcc(info.clone()))
                .padding([8, 24])
                .style(styles::filled)
        },
    ]
    .spacing(8)
    .boxed();

    let filter = text_input("搜索游戏…", &app.game_filter)
        .on_input(Message::GameFilterChanged)
        .style(styles::text_field)
        .width(Length::Fill);

    let needle = app.game_filter.to_lowercase();
    let mut list = column![].spacing(4);
    let mut shown = 0;
    for g in app
        .games
        .iter()
        .filter(|g| needle.is_empty() || g.game_name.to_lowercase().contains(&needle))
        .take(300)
    {
        shown += 1;
        let selected = app.selected_game.as_ref().map(|s| s == g).unwrap_or(false);
        list = list.push(
            button(
                row![
                    if selected { text("✓").color(styles::C::PRIMARY) } else { text(" ") },
                    text(&g.game_name).size(14).width(Length::Fill),
                ]
                .align_y(iced::Alignment::Center),
            )
            .on_press(Message::SelectGame(g.clone()))
            .width(Length::Fill)
            .padding([4, 10])
            .style(if selected {
                styles::picker_selected_row
            } else {
                styles::list_row
            })
            .boxed(),
        );
    }
    if shown == 0 {
        let msg: String = if app.games.is_empty() {
            if app.busy {
                "正在加载游戏列表…".into()
            } else {
                "游戏列表为空".into()
            }
        } else if !needle.is_empty() {
            format!(
                "没有匹配「{}」的游戏（本列表共 {} 个游戏）",
                app.game_filter,
                app.games.len()
            )
        } else {
            "游戏列表为空".into()
        };
        list = list.push(
            text(msg)
                .size(13)
                .color(styles::C::ON_SURFACE_VARIANT)
                .boxed(),
        );
    }

    let game_sel = match &app.selected_game {
        Some(g) => format!(
            "当前选择: {} / {} / {}",
            g.game_name,
            app.selected_zone
                .as_ref()
                .map(|z| z.zone_name.clone())
                .unwrap_or_else(|| "默认区域".into()),
            app.selected_server
                .as_ref()
                .map(|s| s.server_name.clone())
                .unwrap_or_else(|| "默认服务器".into()),
        ),
        // no new selection: fall back to the game shown in the device list
        None => match last_game {
            Some(m) => format!(
                "当前选择: {} / {} / {}（上次加速的游戏）",
                m.game_name, m.zone_name, m.server_name
            ),
            None => "当前选择: 未选择游戏（直接开始将沿用原有设置）".into(),
        },
    };

    let zones: &[Zone] = &app.zones;
    let servers: &[Server] = &app.servers;
    let pick = row![
        pick_list(
            app.selected_zone.clone(),
            zones,
            |z: &Zone| z.zone_name.clone(),
        )
        .on_select(Message::SelectZone)
        .placeholder("区域")
        .padding([8, 14])
        .style(styles::picker),
        pick_list(
            app.selected_server.clone(),
            servers,
            |s: &Server| s.server_name.clone(),
        )
        .on_select(Message::SelectServer)
        .placeholder("服务器")
        .padding([8, 14])
        .style(styles::picker),
    ]
    .spacing(8);

    let is_mobile = matches!(info.kind.as_str(), "android" | "apple" | "ios");
    let other_mobile = app.devices.iter().find(|d| {
        d.deviceinfo.acced
            && matches!(d.deviceinfo.kind.as_str(), "android" | "apple" | "ios")
            && App::device_key(&d.deviceinfo) != key
    });

    // boxed children: the hint row is only present for mobile devices
    let mut body: Vec<iced::Element<'_, Message>> = vec![
        controls,
        text(game_sel).size(14).color(styles::C::ON_SURFACE_VARIANT).boxed(),
    ];
    if is_mobile {
        if let Some(o) = other_mobile {
            let name = if o.deviceinfo.hostname.trim().is_empty() {
                o.name.clone()
            } else {
                o.deviceinfo.hostname.clone()
            };
            body.push(
                container(
                    text(format!(
                        "⚠ 移动端同时只能加速 1 台：{} 正在加速。要加速第二台手机，请用手机上的 UU App。",
                        name
                    ))
                    .size(13),
                )
                .padding([10, 14])
                .width(Length::Fill)
                .style(styles::hint)
                .boxed(),
            );
        }
    }
    body.push(filter.boxed());
    body.push(scrollable(list).height(Length::Fill).style(styles::scroll_bar).boxed());
    body.push(pick.boxed());

    column![head, column(body).spacing(10).height(Length::Fill)]
        .spacing(16)
        .height(Length::Fill)
}

// small helper so unused imports don't break builds
#[allow(dead_code)]
fn _unused(_: Color) {}














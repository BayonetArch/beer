use crate::bindings::get_screen_size;
use std::{
    env::{self},
    error::Error,
    fs::{self, File},
    io::{self, Read, Stdout, Write},
    sync::mpsc::{self, Receiver, Sender},
    thread::{self},
    time::Duration,
};
mod bindings;
pub type Res<T> = Result<T, Box<dyn Error>>;
pub const ESC: &'static str = "\x1b";
const COL_START_POS: u16 = 6;
const ROW_START_POS: u16 = 2;

enum ArrowKey {
    UP,
    DOWN,
    LEFT,
    RIGHT,
    NONE,
}

#[inline]
fn ctrl_key(c: char) -> char {
    (c as u8 & 0x1f) as char
}

#[allow(unreachable_code, unused_labels)]
fn handle_screensize_change(tx: &mut Sender<(u16, u16)>) {
    let mut prev_size = get_screen_size().expect("Could not get terminal size");
    //NOTE: checks on every 100 ms which is kinda expensive

    'screen_loop: loop {
        let current_size = get_screen_size().expect("Could not get terminal size");

        if current_size != prev_size {
            let _ = tx.send(current_size);
            prev_size = current_size;
        }
        sleep_ms!(100);
    }
}

struct RawMode; // using tnis for drop (:
impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = self.disable();
        im_reset_scrollable_region!();
        // im_disable_mouse!();
        let _ = im_leave_alt_screen!();
    }
}

#[rustfmt::skip]
impl RawMode {
    fn new()          -> Self    { Self                         }
    fn enable(&self)  -> Res<()> { bindings::enable_raw_mode()  }
    fn disable(&self) -> Res<()> { bindings::disable_raw_mode() }
}

#[derive(Debug)]
struct Editor {
    screen_cols: u16,
    screen_rows: u16,
    cx: u16,
    cy: u16,
    // HACK:
    #[allow(unused)]
    col_offset: usize,
    row_offset: usize,
    stdout: Stdout,
    appbuf: String,
    file_rows: Option<Vec<String>>,
    welcome: bool,
    #[allow(unused)]
    debug_file: File,
}

impl Editor {
    fn new(appbuf: String, dg_fp: &str, fp: Option<String>) -> Res<Self> {
        let (screen_cols, screen_rows) = get_screen_size()?;
        let stdout = io::stdout();
        let mut welcome = true;

        let mut file_rows = None;
        if let Some(fp) = fp {
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .read(true)
                .open(fp)?;
            let mut fstr = String::new();
            f.read_to_string(&mut fstr)?;
            file_rows = Some(fstr.lines().map(|x| x.to_string()).collect());
            welcome = false;
        }

        let debug_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dg_fp)?;

        Ok(Self {
            screen_cols,
            screen_rows,
            cx: COL_START_POS,
            cy: ROW_START_POS,
            col_offset: 0,
            row_offset: 0,
            stdout,
            appbuf,
            file_rows,
            welcome,
            debug_file,
        })
    }
}

fn update_screensize(e: &mut Editor, rx: &mut Receiver<(u16, u16)>) -> Res<()> {
    match rx.try_recv() {
        Ok(s) => {
            (e.screen_cols, e.screen_rows) = s;
            if e.cy >= e.screen_rows - 1 {
                e.cy = e.screen_rows - 1;
            }
            beer_appendbuf!(e, "{}", ansi_reset_scrollable_region!());
            beer_appendbuf!(e, "{}", ansi_set_scrollable_region!(2, e.screen_rows - 1));
        }
        _ => {}
    }

    Ok(())
}

fn arrow_key_pressed(k: char) -> ArrowKey {
    match k {
        'A' => ArrowKey::UP,
        'B' => ArrowKey::DOWN,
        'C' => ArrowKey::RIGHT,
        'D' => ArrowKey::LEFT,
        _ => ArrowKey::NONE,
    }
}

fn manage_einput(e: &mut Editor) -> Res<bool> {
    let mut is_running = true;

    let mut ch_buf = [0; 1];
    io::stdin().read(&mut ch_buf)?;

    if ch_buf[0] != 0 {
        let ch = ch_buf[0] as char;

        match ch {
            k if k == ctrl_key('q') || k == 'q' => is_running = false,

            k if e.welcome == false && k == '\x1b' => {
                let mut buf = [0u8; 3];
                io::stdin().read(&mut buf)?;

                // // HACK:
                // let total_len = e.file_rows.as_ref().unwrap().len();

                if buf[0] == b'[' {
                    match arrow_key_pressed(buf[1] as char) {
                        ArrowKey::UP => {
                            if e.cy as usize == ROW_START_POS as usize && e.row_offset != 0 {
                                e.row_offset -= 1;
                            } else if e.cy != ROW_START_POS {
                                e.cy -= 1;
                            }
                        }

                        ArrowKey::DOWN => {
                            if e.cy as usize >= e.screen_rows as usize - 1
                            // && e.cy as usize + e.row_offset <= total_len
                            {
                                e.row_offset += 1;
                            } else if e.cy != e.screen_rows - 1 {
                                e.cy += 1;
                            }
                        }

                        ArrowKey::LEFT => {
                            if e.cx != COL_START_POS {
                                e.cx -= 1;
                            }
                        }

                        ArrowKey::RIGHT => {
                            if e.cx != e.screen_cols - 1 {
                                e.cx += 1;
                            }
                        }

                        ArrowKey::NONE => {}
                    }
                }
            }

            'e' => {
                if e.welcome == true {
                    e.welcome = false;
                }
            }
            _ => {}
        }
    }

    Ok(is_running)
}

fn print_welcome_msg(e: &mut Editor) {
    let welcome_msg = "beer v0.1.0";
    let y = e.screen_rows / 2;

    if welcome_msg.len() < e.screen_cols.into() {
        let x = (e.screen_cols - welcome_msg.len() as u16) / 2;
        beer_appendbuf!(e, "{}{welcome_msg}", ansi_move_to!(x, y));
    }

    let welcome_msg = "Press 'e' to start editing! 'q' to quit.";
    if welcome_msg.len() < e.screen_cols.into() {
        let x = (e.screen_cols - welcome_msg.len() as u16) / 2;
        beer_appendbuf!(e, "{}{welcome_msg}", ansi_move_to!(x, y + 1));
    }
}

fn display_bottom_bar(e: &mut Editor, file_name: &str, file_type: &str) {
    let logo = "\x1b[47m  \x1b[30mB  \x1b[0m";

    let bar = format!(
        "\x1b[1;40m{}\x1b[0m",
        " ".repeat(e.screen_cols as usize - 1)
    );

    let fname = format!("\x1b[1;40m[{file_name}]\x1b[0m");
    let ft = format!("\x1b[1;40m\x1b[30m{file_type}\x1b[0m");

    // BAR
    beer_appendbuf!(
        e,
        "{}{}",
        ansi_move_to!(ROW_START_POS, e.screen_rows),
        ansi_clear_current_line!()
    );
    beer_appendbuf!(e, "{}", bar);

    // LOGO
    beer_appendbuf!(e, "{}", ansi_move_to!(1, e.screen_rows));
    beer_appendbuf!(e, "{}", logo);

    // File name
    beer_appendbuf!(e, "{}", ansi_move_to!(logo.len() - 15 + 4, e.screen_rows));
    beer_appendbuf!(e, "{fname}");

    // File type
    beer_appendbuf!(
        e,
        "{}",
        ansi_move_to!(e.screen_cols as usize - file_type.len(), e.screen_rows)
    );
    beer_appendbuf!(e, "{ft}");
}

fn update_display(e: &mut Editor, fp: &str) {
    if e.welcome {
        print_welcome_msg(e);
    } else {
        if let Some(lines) = &e.file_rows {
            for (lc, l) in lines.iter().enumerate() {
                if (e.row_offset..=e.row_offset + e.screen_rows as usize - 1).contains(&lc) {
                    let display_row = lc - e.row_offset;

                    beer_appendbuf!(
                        e,
                        "{}{}\x1b[1;30m{}\x1b[0m",
                        ansi_move_to!(2, ROW_START_POS as usize + display_row),
                        ansi_clear_current_line!(),
                        lc
                    );

                    beer_appendbuf!(
                        e,
                        "{}{}",
                        ansi_move_to!(COL_START_POS, ROW_START_POS as usize + display_row),
                        l
                    );
                }
            }
        }
    }
    display_bottom_bar(e, fp, "unknown");

    beer_appendbuf!(e, "{}", ansi_move_to!(e.cx, e.cy));
}

fn parse_args() -> Option<String> {
    let mut args = env::args();
    let _program_name = args.next().unwrap_or_default();

    args.next()
}

#[allow(unreachable_code, unused_labels)]
fn main() -> Res<()> {
    fatal!("Leaving this for a while.I need to study.");
    let file_path = parse_args();

    let (mut tx, mut rx) = mpsc::channel();
    let appbuf = String::new();
    // NOTE: Clone here
    let fp = file_path.clone().unwrap_or("No Name".to_string());

    let mut e = Editor::new(appbuf, ".dbg.log", file_path)?;
    flogf!(e.debug_file, "Starting beer.....");

    let raw = RawMode::new(); //NOTE:  let needed for drop
    raw.enable()?;
    im_enter_alt_screen!();
    im_set_scrollable_region!(2, e.screen_rows - 1);
    // im_enable_mouse!();

    thread::spawn(move || handle_screensize_change(&mut tx));
    flogf!(e.debug_file, "Spawned 'screensize_change' handler thread");

    'main_loop: loop {
        e.appbuf.clear();
        beer_appendbuf!(e, "{}", ansi_clear_screen!());

        update_screensize(&mut e, &mut rx)?;

        #[rustfmt::skip]
        if !manage_einput(&mut e)? { break };
        update_display(&mut e, &fp);

        beer_flush!(e);
        sleep_ms!(33); // 30fps
    }

    Ok(())
}

pub mod beer_macros {

    #[macro_export]
    macro_rules! beer_appendbuf {
    // write into buffer
    ($e:expr, $($fmt:tt)*) => {{
        use std::fmt::Write as FmtWrite;
        let _ = write!($e.appbuf, $($fmt)*);
    }};
}

    #[macro_export]
    macro_rules! beer_flush {
        ($e:expr) => {{
            use std::io::Write;
            $e.stdout.write_all($e.appbuf.as_bytes()).unwrap();
            $e.stdout.flush().unwrap();
            $e.appbuf.clear();
        }};
    }

    // combined
    #[macro_export]
    macro_rules! beer_flush_and_write {
    ($e:expr, $($fmt:tt)*) => {{
        beer_appendbuf!($e, $($fmt)*);
        beer_flush!($e);
    }};
}

    #[macro_export]
    macro_rules! sleep_ms {
        ($t:expr) => {
            thread::sleep(Duration::from_millis($t))
        };
    }

    /* ANSI code Strings */
    #[macro_export]
    macro_rules! ansi_move_to {
        ($c:expr, $r:expr) => {
            format!("{}[{};{}H", ESC, $r, $c)
        };
    }

    #[macro_export]
    macro_rules! ansi_move_up {
        ($r:expr) => {
            format!("{}[{}A", ESC, $r)
        };
    }

    #[macro_export]
    macro_rules! ansi_move_down {
        ($r:expr) => {
            format!("{}[{}B", ESC, $r)
        };
    }

    #[macro_export]
    macro_rules! ansi_move_right {
        ($c:expr) => {
            format!("{}[{}C", ESC, $c)
        };
    }

    #[macro_export]
    macro_rules! ansi_move_left {
        ($c:expr) => {
            format!("{}[{}D", ESC, $c)
        };
    }

    #[macro_export]
    macro_rules! ansi_clear_screen {
        () => {
            format!("{}[2J{}[H", ESC, ESC)
        };
    }

    #[macro_export]
    macro_rules! ansi_clear_current_line {
        () => {
            format!("{}[2K", ESC)
        };
    }

    #[macro_export]
    macro_rules! save_cursor_pos {
        () => {
            format!("{}[s", ESC)
        };
    }

    #[macro_export]
    macro_rules! restore_cursor_pos {
        () => {
            format!("{}[u", ESC)
        };
    }

    #[macro_export]
    macro_rules! ansi_set_scrollable_region {
        ($top:expr, $bottom:expr) => {
            format!("{}[{};{}r", ESC, $top, $bottom)
        };
    }

    #[macro_export]
    macro_rules! ansi_reset_scrollable_region {
        () => {
            format!("{}[r", ESC)
        };
    }
    /*  immediate commands*/
    #[macro_export]
    macro_rules! im_clear_screen {
        () => {{
            let _ = write!(std::io::stdout(), "{}[2J{}[H", ESC, ESC);

            let _ = std::io::stdout().flush();
        }};
    }

    #[macro_export]
    macro_rules! im_set_scrollable_region {
        ($top:expr, $bottom:expr) => {{
            let _ = write!(std::io::stdout(), "{}[{};{}r", ESC, $top, $bottom);
            let _ = std::io::stdout().flush();
        }};
    }

    #[macro_export]
    macro_rules! im_reset_scrollable_region {
        () => {{
            let _ = write!(std::io::stdout(), "{}[r", ESC);
            let _ = std::io::stdout().flush();
        }};
    }

    #[macro_export]
    macro_rules! im_enter_alt_screen {
        () => {{
            let _ = write!(std::io::stdout(), "{}[?1049h", ESC);
            let _ = std::io::stdout().flush();
        }};
    }

    #[macro_export]
    macro_rules! im_leave_alt_screen {
        () => {{
            let _ = write!(std::io::stdout(), "{}[?1049l", ESC);
            let _ = std::io::stdout().flush();
        }};
    }

    #[macro_export]
    macro_rules! im_enable_mouse {
        () => {{
            let _ = write!(std::io::stdout(), "{}[?1002h{}[?1006h ", ESC, ESC);
            let _ = std::io::stdout().flush();
        }};
    }

    #[macro_export]
    macro_rules! im_disable_mouse {
        () => {{
            let _ = write!(std::io::stdout(), "{}[?1002l{}[?1006l ", ESC, ESC);
            let _ = std::io::stdout().flush();
        }};
    }

    #[macro_export]
    macro_rules! fatal {
        ($($fmt:tt)*) => {{
            return Err(format!("{}:{} {}", file!(), line!(), format!($($fmt)*)).into())
        }};
    }

    #[macro_export]
    macro_rules! flogf {
    ($f:expr,$($fmt:tt)*) => {{
        use crate::bindings::current_str_date;
        let t = current_str_date().expect("Could not get current time.");
        let _ = writeln!($f, "{} {}",t,format!($($fmt)*));

    }};
}
}

// TODO: make  'update_screensize' to use WINCH signal
// TODO: keep track of lines in files
// TODO: responsivly display the rows and cols
// TODO: mouse support

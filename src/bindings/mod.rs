use crate::{Res, fatal};
use std::{
    ffi::{c_int, c_uchar, c_uint, c_ushort, c_void},
    mem::zeroed,
    sync::Mutex,
};

type TcFlagT = c_uint;
type CcT = c_uchar;
const NCCS: c_uint = 19;
const TCGETS: c_int = 0x5401;
const TCSETS: c_int = 0x5402;
const TIOCGWINSZ: c_int = 21523;
const VTIME: c_int = 5;
const VMIN: c_int = 6;
// const SIGWINCH: c_int = 28;

static O_TS: Mutex<TermiOS> = Mutex::new(unsafe { zeroed() });

#[derive(Clone, Copy)]
#[repr(C)]
struct TermiOS {
    c_iflag: TcFlagT,
    c_oflag: TcFlagT,
    c_cflag: TcFlagT,
    c_lflag: TcFlagT,
    c_line: CcT,
    c_cc: [CcT; NCCS as usize],
}

unsafe extern "C" {
    fn ioctl(fd: c_int, request: c_int, ...) -> c_int;
    fn cfmakeraw(s: *mut TermiOS) -> c_void;
    // fn _signal(signal: c_int, sig_handler: extern "C" fn(c_int)) -> extern "C" fn(c_int);
    // fn write!(, "")
}

fn get_term_attr(ts: &mut TermiOS) -> Res<()> {
    let ret = unsafe { ioctl(0, TCGETS, ts) };
    if ret == -1 {
        fatal!("Could not get terminal attributes.('ioctl' syscall failed)");
    }
    Ok(())
}

fn set_term_attr(ts: &mut TermiOS) -> Res<()> {
    let ret = unsafe { ioctl(0, TCSETS, ts) };
    if ret == -1 {
        fatal!("Could not set terminal attributes.('ioctl' syscall failed)");
    }
    Ok(())
}

fn make_raw(ts: &mut TermiOS) {
    unsafe {
        cfmakeraw(ts);
        ts.c_cc[VMIN as usize] = 0; // set  mimimum bytes for read()
        ts.c_cc[VTIME as usize] = 1; // minimum time
    };
}

pub fn enable_raw_mode() -> Res<()> {
    let mut o_ts_guard = O_TS.lock()?;
    get_term_attr(&mut *o_ts_guard)?; // get default attrs and set to global var

    let mut raw = *o_ts_guard; // override the default to make raw mode
    make_raw(&mut raw);

    set_term_attr(&mut raw)?;

    Ok(())
}

pub fn disable_raw_mode() -> Res<()> {
    let mut o_ts_guard = O_TS.lock()?; // original

    set_term_attr(&mut *o_ts_guard)?;
    Ok(())
}

#[repr(C)]
struct WinSize {
    ws_row: c_ushort,
    ws_col: c_ushort,
    ws_xpixel: c_ushort,
    ws_ypixel: c_ushort,
}

#[rustfmt::skip]
/// returns (col, row)
pub fn get_screen_size() -> Res<(u16, u16)> {
    let mut ws: WinSize = unsafe { zeroed() };

    let ret = unsafe { ioctl(0, TIOCGWINSZ, &mut ws) };
    if ret != 0 { fatal!("Could not get window size. 'ioctl' syscall failed") }

    Ok((ws.ws_col, ws.ws_row))
}

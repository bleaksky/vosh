//! Telnet protocol byte constants. Values per RFC 854 plus the option numbers
//! we recognize for MUD use.

pub const IAC: u8 = 255;
pub const DONT: u8 = 254;
pub const DO: u8 = 253;
pub const WONT: u8 = 252;
pub const WILL: u8 = 251;
pub const SB: u8 = 250;
pub const GA: u8 = 249;
pub const EL: u8 = 248;
pub const EC: u8 = 247;
pub const AYT: u8 = 246;
pub const AO: u8 = 245;
pub const IP: u8 = 244;
pub const BRK: u8 = 243;
pub const DM: u8 = 242;
pub const NOP: u8 = 241;
pub const SE: u8 = 240;
pub const EOR: u8 = 239;

pub mod option {
    pub const BINARY: u8 = 0;
    pub const ECHO: u8 = 1;
    pub const SUPPRESS_GO_AHEAD: u8 = 3;
    pub const STATUS: u8 = 5;
    pub const TTYPE: u8 = 24;
    pub const EOR: u8 = 25;
    pub const NAWS: u8 = 31;
    pub const LINEMODE: u8 = 34;
    pub const NEW_ENVIRON: u8 = 39;
    pub const CHARSET: u8 = 42;
    pub const MSDP: u8 = 69;
    pub const MSSP: u8 = 70;
    pub const MCCP2: u8 = 86;
    pub const MCCP3: u8 = 87;
    pub const MXP: u8 = 91;
    pub const ATCP: u8 = 200;
    pub const GMCP: u8 = 201;
}

pub mod ttype {
    pub const IS: u8 = 0;
    pub const SEND: u8 = 1;
}

pub mod charset {
    pub const REQUEST: u8 = 1;
    pub const ACCEPTED: u8 = 2;
    pub const REJECTED: u8 = 3;
}

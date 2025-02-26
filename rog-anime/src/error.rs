use std::error::Error;
use std::fmt;

use gif::DecodingError;
use png_pong::decode::Error as PngError;

pub type Result<T> = std::result::Result<T, AnimeError>;

#[derive(Debug)]
pub enum AnimeError {
    NoFrames,
    Io(std::io::Error),
    Png(PngError),
    Gif(DecodingError),
    Format,
    /// The input was incorrect size, expected size is `IncorrectSize(width,
    /// height)`
    IncorrectSize(u32, u32),
    Dbus(String),
    Udev(String, std::io::Error),
    NoDevice,
    UnsupportedDevice,
    InvalidBrightness(f32),
    DataBufferLength,
    PixelGifWidth(usize),
    PixelGifHeight(usize),
    ParseError(String)
}

impl fmt::Display for AnimeError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnimeError::ParseError(e) => write!(f, "Could not parse {e}"),
            AnimeError::NoFrames => write!(f, "No frames in PNG"),
            AnimeError::Io(e) => write!(f, "Could not open: {}", e),
            AnimeError::Png(e) => write!(f, "PNG error: {}", e),
            AnimeError::Gif(e) => write!(f, "GIF error: {}", e),
            AnimeError::Format => write!(f, "PNG file is not 8bit greyscale"),
            AnimeError::IncorrectSize(width, height) => write!(
                f,
                "The input image size is incorrect, expected {}x{}",
                width, height
            ),
            AnimeError::Dbus(detail) => write!(f, "{}", detail),
            AnimeError::Udev(deets, error) => write!(f, "udev {}: {}", deets, error),
            AnimeError::NoDevice => write!(f, "No AniMe Matrix device found"),
            AnimeError::DataBufferLength => write!(
                f,
                "The data buffer was incorrect length for generating USB packets"
            ),
            AnimeError::UnsupportedDevice => write!(f, "Unsupported AniMe Matrix device found"),
            AnimeError::InvalidBrightness(bright) => write!(
                f,
                "Image brightness must be between 0.0 and 1.0 (inclusive), was {}",
                bright
            ),
            AnimeError::PixelGifWidth(n) => {
                write!(f, "The gif used for pixel-perfect gif is is wider than {n}")
            }
            AnimeError::PixelGifHeight(n) => write!(
                f,
                "The gif used for pixel-perfect gif is is taller than {n}"
            )
        }
    }
}

impl Error for AnimeError {}

impl From<std::io::Error> for AnimeError {
    #[inline]
    fn from(err: std::io::Error) -> Self {
        AnimeError::Io(err)
    }
}

impl From<PngError> for AnimeError {
    #[inline]
    fn from(err: PngError) -> Self {
        AnimeError::Png(err)
    }
}

impl From<DecodingError> for AnimeError {
    #[inline]
    fn from(err: DecodingError) -> Self {
        AnimeError::Gif(err)
    }
}

impl From<AnimeError> for zbus::fdo::Error {
    #[inline]
    fn from(err: AnimeError) -> Self {
        zbus::fdo::Error::Failed(format!("{}", err))
    }
}

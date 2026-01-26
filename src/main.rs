/*!
===============================================================================
NDK Color Conversion Tool (NCLR)
-------------------------------------------------------------------------------
ICC-aware color conversion and bit-depth transformation using LittleCMS 2
Via the safe `lcms2` Rust crate.

Author: Jan Houserek
License: GPL-3.0-or-later
===============================================================================
*/

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, ValueEnum};
use image::GenericImageView;
use lcms2::{Flags, Intent, PixelFormat, Profile, Transform};
use rgb::{RGB16, RGB8};
use std::borrow::Cow;
use std::cmp::min;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;
use rayon::prelude::*;

use tiff::encoder::{colortype, Rational, TiffEncoder, TiffValue};
use tiff::tags::{ResolutionUnit, Tag, Type as TiffType};

#[derive(Debug, Copy, Clone, ValueEnum)]
enum RenderIntent {
    Perceptual,
    Relative,
    Absolute,
    Saturation,
}
impl From<RenderIntent> for Intent {
    fn from(v: RenderIntent) -> Self {
        match v {
            RenderIntent::Perceptual => Intent::Perceptual,
            RenderIntent::Relative => Intent::RelativeColorimetric,
            RenderIntent::Absolute => Intent::AbsoluteColorimetric,
            RenderIntent::Saturation => Intent::Saturation,
        }
    }
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum BitDepth {
    B8,
    B16,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum DetectInputIcc {
    Auto,
    Srgb,
    File,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum ToneMap {
    None,
    Gamma,
    Perceptual,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum Preset {
    /// Convenience preset for NDK Master Copy
    #[value(name = "ndk-mc")]
    NdkMc,
    /// Convenience preset for NDK User Copy I (books/periodicals)
    #[value(name = "ndk-uc-i")]
    NdkUcI,
    /// Convenience preset for NDK User Copy II (maps/manuscripts/old prints)
    #[value(name = "ndk-uc-ii")]
    NdkUcII,
}

#[derive(Parser, Debug)]
#[command(
    name = "nclr",
    version,
    about = "NDK-oriented ICC color conversion and 16↔8 bit-depth conversion using LittleCMS2 (lcms2 crate)."
)]
struct Args {
    /// High-level convenience preset that fills recommended defaults.
    /// Explicit options always take precedence.
    #[arg(long, value_enum)]
    preset: Option<Preset>,

    /// Input image (TIFF/PNG/JPEG...). For 16-bit workflows use TIFF/PNG.
    #[arg(short = 'i', long)]
    input: PathBuf,

    /// Output path. If INPUT is a file, this must be a file path (extension selects format).
    /// If INPUT is a directory, this must be an output directory path.
    #[arg(short = 'o', long)]
    output: PathBuf,

    /// If INPUT is a directory, scan it (and optionally its subdirectories) for images.
    /// Supported extensions: tif, tiff, png, jpg, jpeg.
    #[arg(short = 'r', long, default_value_t = false)]
    recursive: bool,

    /// When INPUT is a directory, choose output extension for generated files.
    /// Default: "tif".
    #[arg(long, default_value = "tif")]
    out_ext: String,

    /// When INPUT is a directory, append this suffix to each output filename stem.
    /// Example: "_uc-ii".
    #[arg(long, default_value = "")]
    suffix: String,

    /// Overwrite existing output files.
    #[arg(long, default_value_t = false)]
    overwrite: bool,

    /// Parallel jobs for directory conversion (0 = auto).
    #[arg(long, default_value_t = 0)]
    jobs: usize,

    /// How to pick input ICC.
    #[arg(long, value_enum, default_value_t = DetectInputIcc::Auto)]
    detect_input_icc: DetectInputIcc,

    /// ICC profile file used when --detect-input-icc=file.
    #[arg(long)]
    input_icc_file: Option<PathBuf>,

    /// Output ICC profile file.
    ///
    /// Policy:
    /// - UC-I: ignored unless --force-out-icc
    /// - UC-II: default is sRGB if not set
    /// - MC: default is "preserve embedded input ICC" if present; otherwise sRGB
    #[arg(long)]
    out_icc: Option<PathBuf>,

    /// Rendering intent (ICC transform).
    #[arg(long, value_enum)]
    intent: Option<RenderIntent>,

    /// Black Point Compensation (BPC). Default: true.
    #[arg(long, default_value_t = true)]
    bpc: bool,

    /// Output bit depth (overrides policy defaults).
    #[arg(long, value_enum)]
    out_depth: Option<BitDepth>,

    /// Tone mapping used when downconverting 16->8 (applied AFTER ICC transform).
    #[arg(long, value_enum)]
    tone_map: Option<ToneMap>,

    /// Apply Floyd–Steinberg dithering after 16->8 quantization.
    #[arg(long)]
    dither: Option<bool>,

    /// Write the output ICC profile as a sidecar next to each output image.
    ///
    /// The sidecar path is derived from the output image path by changing the extension to `.icc`.
    #[arg(long, default_value_t = false)]
    write_icc: bool,

    /// Override NDK policy and allow output ICC for UC-I.
    #[arg(long, default_value_t = false)]
    force_out_icc: bool,

    /// Print ICC diagnostics (profile sizes, version).
    #[arg(long, default_value_t = false)]
    debug_icc: bool,

    /// If set, do not apply ICC transform; only convert bit depth / drop alpha.
    #[arg(long, default_value_t = false)]
    no_icc: bool,
}

#[derive(Debug, Copy, Clone)]
struct Effective {
    preset: Preset,
    out_depth: BitDepth,
    intent: RenderIntent,
    tone_map: ToneMap,
    dither: bool,
    bpc: bool,
}

/// Apply preset defaults, but do NOT override explicit user options.
fn compute_effective(args: &Args) -> Effective {
    // Default preset is NDK UC-II if not specified
    let preset = args.preset.unwrap_or(Preset::NdkUcII);

    // Base defaults
    let mut intent = args.intent.unwrap_or(RenderIntent::Perceptual);
    let mut tone_map = args.tone_map.unwrap_or(ToneMap::None);
    let mut dither = args.dither.unwrap_or(false);
    let bpc = args.bpc;

    // Preset-specific defaults (only fill what the user didn't specify)
    match preset {
        Preset::NdkMc | Preset::NdkUcI => {
            if args.intent.is_none() {
                intent = RenderIntent::Perceptual;
            }
            if args.tone_map.is_none() {
                tone_map = ToneMap::None;
            }
            if args.dither.is_none() {
                dither = false;
            }
        }
        Preset::NdkUcII => {
            if args.intent.is_none() {
                intent = RenderIntent::Perceptual;
            }
            // Conservative defaults; you can change these to perceptual+dither for UC-II
            if args.tone_map.is_none() {
                tone_map = ToneMap::None;
            }
            if args.dither.is_none() {
                dither = false;
            }
        }
    }

    // Output depth default depends on the preset
    let out_depth = args.out_depth.unwrap_or_else(|| match preset {
        Preset::NdkMc => BitDepth::B16,
        _ => BitDepth::B8,
    });

    Effective {
        preset,
        out_depth,
        intent,
        tone_map,
        dither,
        bpc,
    }
}

// ---------------- TIFF metadata (ICC + resolution) ----------------

#[derive(Clone)]
struct TiffMeta {
    icc: Option<Vec<u8>>,
    x_res: Option<Rational>,
    y_res: Option<Rational>,
    unit: Option<ResolutionUnit>,
}

fn file_ext_lower(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn is_tiff_path(p: &Path) -> bool {
    matches!(file_ext_lower(p).as_str(), "tif" | "tiff")
}

fn read_exact_at(f: &mut File, off: u64, buf: &mut [u8]) -> Result<()> {
    f.seek(SeekFrom::Start(off))
        .with_context(|| format!("Seek @ {off}"))?;
    f.read_exact(buf)
        .with_context(|| format!("Read {} bytes @ {off}", buf.len()))?;
    Ok(())
}

fn read_u16_endian(b: [u8; 2], le: bool) -> u16 {
    if le {
        u16::from_le_bytes(b)
    } else {
        u16::from_be_bytes(b)
    }
}
fn read_u32_endian(b: [u8; 4], le: bool) -> u32 {
    if le {
        u32::from_le_bytes(b)
    } else {
        u32::from_be_bytes(b)
    }
}
fn read_u64_endian(b: [u8; 8], le: bool) -> u64 {
    if le {
        u64::from_le_bytes(b)
    } else {
        u64::from_be_bytes(b)
    }
}

/// Minimal TIFF/BigTIFF reader for:
/// - ICCProfile (34675)
/// - XResolution (282), YResolution (283), ResolutionUnit (296)
///
/// Reads only IFD0 + referenced value blocks.
fn read_tiff_meta(path: &Path) -> Result<TiffMeta> {
    let mut f = File::open(path).with_context(|| format!("Open TIFF: {}", path.display()))?;

    // Header
    let mut head = [0u8; 16];
    read_exact_at(&mut f, 0, &mut head)?;

    let le = match &head[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => bail!("Not a TIFF (bad endian marker)"),
    };

    let magic = read_u16_endian([head[2], head[3]], le);

    // TIFF type sizes (subset we need)
    fn type_size(t: u16) -> Option<u64> {
        match t {
            1 => Some(1),  // BYTE
            3 => Some(2),  // SHORT
            4 => Some(4),  // LONG
            5 => Some(8),  // RATIONAL (2x u32)
            7 => Some(1),  // UNDEFINED
            16 => Some(8), // LONG8 (BigTIFF)
            _ => None,
        }
    }

    let mut meta = TiffMeta {
        icc: None,
        x_res: None,
        y_res: None,
        unit: None,
    };

    let icc_tag: u16 = 34675;
    let xres_tag: u16 = 282;
    let yres_tag: u16 = 283;
    let unit_tag: u16 = 296;

    if magic == 42 {
        // Classic TIFF
        let ifd0_off = read_u32_endian([head[4], head[5], head[6], head[7]], le) as u64;

        let mut nbuf = [0u8; 2];
        read_exact_at(&mut f, ifd0_off, &mut nbuf)?;
        let n = read_u16_endian(nbuf, le) as u64;

        let mut ent_off = ifd0_off + 2;
        for _ in 0..n {
            let mut ent = [0u8; 12];
            read_exact_at(&mut f, ent_off, &mut ent)?;
            ent_off += 12;

            let tag = read_u16_endian([ent[0], ent[1]], le);
            let ty = read_u16_endian([ent[2], ent[3]], le);
            let count = read_u32_endian([ent[4], ent[5], ent[6], ent[7]], le) as u64;
            let value_or_off = read_u32_endian([ent[8], ent[9], ent[10], ent[11]], le) as u64;

            let tsz = match type_size(ty) {
                Some(s) => s,
                None => continue,
            };
            let bytes_len = count.saturating_mul(tsz);

            let get_bytes = |f: &mut File| -> Result<Vec<u8>> {
                if bytes_len == 0 {
                    return Ok(Vec::new());
                }
                if bytes_len <= 4 {
                    Ok(ent[8..8 + (bytes_len as usize)].to_vec())
                } else {
                    let mut v = vec![0u8; bytes_len as usize];
                    read_exact_at(f, value_or_off, &mut v)?;
                    Ok(v)
                }
            };

            match tag {
                t if t == icc_tag => {
                    let b = get_bytes(&mut f)?;
                    if !b.is_empty() {
                        meta.icc = Some(b);
                    }
                }
                t if t == xres_tag => {
                    let b = get_bytes(&mut f)?;
                    if b.len() >= 8 {
                        let n = read_u32_endian([b[0], b[1], b[2], b[3]], le);
                        let d = read_u32_endian([b[4], b[5], b[6], b[7]], le);
                        if d != 0 {
                            meta.x_res = Some(Rational { n, d });
                        }
                    }
                }
                t if t == yres_tag => {
                    let b = get_bytes(&mut f)?;
                    if b.len() >= 8 {
                        let n = read_u32_endian([b[0], b[1], b[2], b[3]], le);
                        let d = read_u32_endian([b[4], b[5], b[6], b[7]], le);
                        if d != 0 {
                            meta.y_res = Some(Rational { n, d });
                        }
                    }
                }
                t if t == unit_tag => {
                    let b = get_bytes(&mut f)?;
                    if b.len() >= 2 {
                        let u = read_u16_endian([b[0], b[1]], le);
                        meta.unit = Some(match u {
                            2 => ResolutionUnit::Inch,
                            3 => ResolutionUnit::Centimeter,
                            _ => ResolutionUnit::None,
                        });
                    }
                }
                _ => {}
            }
        }
    } else if magic == 43 {
        // BigTIFF
        let off_size = read_u16_endian([head[4], head[5]], le);
        if off_size != 8 {
            bail!("Unsupported BigTIFF offset size: {}", off_size);
        }
        let ifd0_off = read_u64_endian(
            [
                head[8], head[9], head[10], head[11], head[12], head[13], head[14], head[15],
            ],
            le,
        );

        let mut nbuf = [0u8; 8];
        read_exact_at(&mut f, ifd0_off, &mut nbuf)?;
        let n = read_u64_endian(nbuf, le);

        let mut ent_off = ifd0_off + 8;
        for _ in 0..n {
            let mut ent = [0u8; 20];
            read_exact_at(&mut f, ent_off, &mut ent)?;
            ent_off += 20;

            let tag = read_u16_endian([ent[0], ent[1]], le);
            let ty = read_u16_endian([ent[2], ent[3]], le);
            let count = read_u64_endian(
                [ent[4], ent[5], ent[6], ent[7], ent[8], ent[9], ent[10], ent[11]],
                le,
            );
            let value_or_off = read_u64_endian(
                [ent[12], ent[13], ent[14], ent[15], ent[16], ent[17], ent[18], ent[19]],
                le,
            );

            let tsz = match type_size(ty) {
                Some(s) => s,
                None => continue,
            };
            let bytes_len = count.saturating_mul(tsz);

            let get_bytes = |f: &mut File| -> Result<Vec<u8>> {
                if bytes_len == 0 {
                    return Ok(Vec::new());
                }
                if bytes_len <= 8 {
                    Ok(ent[12..12 + (bytes_len as usize)].to_vec())
                } else {
                    let mut v = vec![0u8; bytes_len as usize];
                    read_exact_at(f, value_or_off, &mut v)?;
                    Ok(v)
                }
            };

            match tag {
                t if t == icc_tag => {
                    let b = get_bytes(&mut f)?;
                    if !b.is_empty() {
                        meta.icc = Some(b);
                    }
                }
                t if t == xres_tag => {
                    let b = get_bytes(&mut f)?;
                    if b.len() >= 8 {
                        let n = read_u32_endian([b[0], b[1], b[2], b[3]], le);
                        let d = read_u32_endian([b[4], b[5], b[6], b[7]], le);
                        if d != 0 {
                            meta.x_res = Some(Rational { n, d });
                        }
                    }
                }
                t if t == yres_tag => {
                    let b = get_bytes(&mut f)?;
                    if b.len() >= 8 {
                        let n = read_u32_endian([b[0], b[1], b[2], b[3]], le);
                        let d = read_u32_endian([b[4], b[5], b[6], b[7]], le);
                        if d != 0 {
                            meta.y_res = Some(Rational { n, d });
                        }
                    }
                }
                t if t == unit_tag => {
                    let b = get_bytes(&mut f)?;
                    if b.len() >= 2 {
                        let u = read_u16_endian([b[0], b[1]], le);
                        meta.unit = Some(match u {
                            2 => ResolutionUnit::Inch,
                            3 => ResolutionUnit::Centimeter,
                            _ => ResolutionUnit::None,
                        });
                    }
                }
                _ => {}
            }
        }
    } else {
        bail!("Unknown TIFF magic: {}", magic);
    }

    Ok(meta)
}

// ---------------- ICC detection helpers (TIFF/JPEG) ----------------

/// Read embedded ICC from JPEG APP2 ICC_PROFILE segments (minimal parser).
fn read_icc_from_jpeg(path: &Path) -> Result<Option<Vec<u8>>> {
    let mut data = Vec::new();
    fs::File::open(path)?.read_to_end(&mut data)?;

    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return Ok(None);
    }

    let mut chunks: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut i = 2;

    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        i += 2;

        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        if i + 2 > data.len() {
            break;
        }

        let seg_len = u16::from_be_bytes([data[i], data[i + 1]]) as usize;
        i += 2;
        if seg_len < 2 || i + (seg_len - 2) > data.len() {
            break;
        }

        let seg = &data[i..i + (seg_len - 2)];
        i += seg_len - 2;

        if marker == 0xE2 {
            const MAGIC: &[u8] = b"ICC_PROFILE\0";
            if seg.len() > MAGIC.len() + 2 && &seg[..MAGIC.len()] == MAGIC {
                let seq_no = seg[MAGIC.len()];
                let payload = seg[MAGIC.len() + 2..].to_vec();
                chunks.push((seq_no, payload));
            }
        }
    }

    if chunks.is_empty() {
        return Ok(None);
    }

    chunks.sort_by_key(|(n, _)| *n);
    let mut out = Vec::new();
    for (_, part) in chunks {
        out.extend_from_slice(&part);
    }
    Ok(if out.is_empty() { None } else { Some(out) })
}

fn pick_input_profile(args: &Args, tiff_meta: Option<&TiffMeta>) -> Result<Profile> {
    match args.detect_input_icc {
        DetectInputIcc::Srgb => Ok(Profile::new_srgb()),
        DetectInputIcc::File => {
            let p = args
                .input_icc_file
                .as_deref()
                .context("--detect-input-icc=file requires --input-icc-file")?;
            Ok(Profile::new_file(p)?)
        }
        DetectInputIcc::Auto => {
            let ext = file_ext_lower(&args.input);

            // Prefer TIFF meta if available (cheap, no full decode)
            if ext == "tif" || ext == "tiff" {
                if let Some(m) = tiff_meta {
                    if let Some(bytes) = &m.icc {
                        return Ok(Profile::new_icc(bytes)?);
                    }
                }
            }

            let icc_bytes = if ext == "jpg" || ext == "jpeg" {
                read_icc_from_jpeg(&args.input)?
            } else {
                None
            };

            if let Some(bytes) = icc_bytes {
                Ok(Profile::new_icc(&bytes)?)
            } else {
                Ok(Profile::new_srgb())
            }
        }
    }
}

/// Output profile policy:
/// - UC-I: ICC OFF (unless force_out_icc)
/// - UC-II: ICC ON (default sRGB unless out_icc specified)
/// - MC: ICC ON:
///     - if out_icc specified => that
///     - else if embedded input ICC exists => preserve it (do NOT force sRGB)
///     - else => sRGB
fn pick_output_profile_with_policy(
    args: &Args,
    preset: Preset,
    in_prof: &Profile,
    in_icc_bytes: Option<&[u8]>,
) -> Result<Option<Profile>> {
    match preset {
        Preset::NdkUcI => {
            if args.force_out_icc {
                let p = match args.out_icc.as_deref() {
                    Some(path) => Profile::new_file(path)?,
                    None => Profile::new_srgb(),
                };
                Ok(Some(p))
            } else {
                Ok(None)
            }
        }
        Preset::NdkUcII => {
            let p = match args.out_icc.as_deref() {
                Some(path) => Profile::new_file(path)?,
                None => Profile::new_srgb(),
            };
            Ok(Some(p))
        }
        Preset::NdkMc => {
            if let Some(path) = args.out_icc.as_deref() {
                return Ok(Some(Profile::new_file(path)?));
            }
            if let Some(b) = in_icc_bytes {
                return Ok(Some(Profile::new_icc(b)?));
            }
            // Fallback: preserve "whatever in_prof is" (likely sRGB if no embedded)
            // but we still return an explicit profile:
            let b = in_prof.icc().ok();
            if let Some(bb) = b {
                return Ok(Some(Profile::new_icc(&bb)?));
            }
            Ok(Some(Profile::new_srgb()))
        }
    }
}

// ---------------- Image decode helpers ----------------

fn load_rgb16(path: &Path) -> Result<(u32, u32, Vec<RGB16>)> {
    // Disable image crate decoding limits (huge TIFFs)
    let mut reader = image::ImageReader::open(path)
        .with_context(|| format!("Open input: {}", path.display()))?
        .with_guessed_format()
        .context("Guess image format")?;
    reader.no_limits();

    let img = reader.decode().context("Decode image")?;
    let (w, h) = img.dimensions();

    // Convert to RGB16
    let raw = img.to_rgb16().into_raw();
    let pix = raw
        .chunks_exact(3)
        .map(|c| RGB16::new(c[0], c[1], c[2]))
        .collect::<Vec<_>>();

    Ok((w, h, pix))
}

// ---------------- Quantization + tonemapping + dithering ----------------

#[inline]
fn apply_tonemap_norm(x: f32, tone: ToneMap) -> f32 {
    let x = x.clamp(0.0, 1.0);
    match tone {
        ToneMap::None => x,
        ToneMap::Gamma => x.powf(1.0 / 2.2),
        ToneMap::Perceptual => x.sqrt(),
    }
}

fn quantize_rgb16_to_rgb8_stream_dither(
    pix: &[RGB16],
    w: u32,
    h: u32,
    tone: ToneMap,
    dither: bool,
) -> Vec<RGB8> {
    let w = w as usize;
    let h = h as usize;

    let mut out = vec![RGB8::new(0, 0, 0); w * h];

    if !dither {
        for i in 0..(w * h) {
            let p = pix[i];
            let r = (apply_tonemap_norm(p.r as f32 / 65535.0, tone) * 255.0 + 0.5) as i32;
            let g = (apply_tonemap_norm(p.g as f32 / 65535.0, tone) * 255.0 + 0.5) as i32;
            let b = (apply_tonemap_norm(p.b as f32 / 65535.0, tone) * 255.0 + 0.5) as i32;
            out[i] = RGB8::new(
                r.clamp(0, 255) as u8,
                g.clamp(0, 255) as u8,
                b.clamp(0, 255) as u8,
            );
        }
        return out;
    }

    // Floyd–Steinberg with scanline error buffers:
    // store errors as i32 in 1/16 units, per channel.
    let mut err_cur = vec![0i32; w * 3];
    let mut err_nxt = vec![0i32; w * 3];

    for y in 0..h {
        err_nxt.fill(0);

        for x in 0..w {
            let idx = y * w + x;
            let p = pix[idx];

            let base_r = (apply_tonemap_norm(p.r as f32 / 65535.0, tone) * 255.0).round() as i32;
            let base_g = (apply_tonemap_norm(p.g as f32 / 65535.0, tone) * 255.0).round() as i32;
            let base_b = (apply_tonemap_norm(p.b as f32 / 65535.0, tone) * 255.0).round() as i32;

            let eoff = x * 3;

            let rr = base_r + (err_cur[eoff + 0] / 16);
            let gg = base_g + (err_cur[eoff + 1] / 16);
            let bb = base_b + (err_cur[eoff + 2] / 16);

            let qr = rr.clamp(0, 255);
            let qg = gg.clamp(0, 255);
            let qb = bb.clamp(0, 255);

            out[idx] = RGB8::new(qr as u8, qg as u8, qb as u8);

            // quantization error (scaled *16)
            let er = (rr - qr) * 16;
            let eg = (gg - qg) * 16;
            let eb = (bb - qb) * 16;

            // distribute: right (7/16), down-left (3/16), down (5/16), down-right (1/16)
            if x + 1 < w {
                err_cur[(x + 1) * 3 + 0] += (er * 7) / 16;
                err_cur[(x + 1) * 3 + 1] += (eg * 7) / 16;
                err_cur[(x + 1) * 3 + 2] += (eb * 7) / 16;
            }
            if y + 1 < h {
                if x > 0 {
                    err_nxt[(x - 1) * 3 + 0] += (er * 3) / 16;
                    err_nxt[(x - 1) * 3 + 1] += (eg * 3) / 16;
                    err_nxt[(x - 1) * 3 + 2] += (eb * 3) / 16;
                }
                err_nxt[x * 3 + 0] += (er * 5) / 16;
                err_nxt[x * 3 + 1] += (eg * 5) / 16;
                err_nxt[x * 3 + 2] += (eb * 5) / 16;

                if x + 1 < w {
                    err_nxt[(x + 1) * 3 + 0] += (er * 1) / 16;
                    err_nxt[(x + 1) * 3 + 1] += (eg * 1) / 16;
                    err_nxt[(x + 1) * 3 + 2] += (eb * 1) / 16;
                }
            }
        }

        std::mem::swap(&mut err_cur, &mut err_nxt);
        err_cur.fill(0);
    }

    out
}

// ---------------- TIFF writing with ICC + DPI ----------------

/// Ensure ICC tag (34675) is written as TIFF type UNDEFINED (7), not BYTE (1),
/// to satisfy strict validators like JHOVE TIFF-hul.
struct UndefinedBytes<'a>(&'a [u8]);

impl<'a> TiffValue for UndefinedBytes<'a> {
    const BYTE_LEN: u8 = 1;
    const FIELD_TYPE: TiffType = TiffType::UNDEFINED;

    fn count(&self) -> usize {
        self.0.len()
    }

    fn data(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(self.0)
    }
}

#[inline]
fn rat(v: &Rational) -> Rational {
    Rational { n: v.n, d: v.d }
}

fn normalize_resolution(meta: Option<&TiffMeta>) -> (ResolutionUnit, Rational, Rational) {
    // Defaults if nothing known:
    let mut unit = ResolutionUnit::Inch;
    let mut xr = Rational { n: 600, d: 1 };
    let mut yr = Rational { n: 600, d: 1 };

    if let Some(m) = meta {
        if let Some(u) = m.unit {
            // If input says None but has values, we still prefer Inch for "dpi" semantics.
            unit = if matches!(u, ResolutionUnit::None) {
                ResolutionUnit::Inch
            } else {
                u
            };
        }

        if let Some(x) = m.x_res.as_ref() {
            xr = rat(x);
        }
        if let Some(y) = m.y_res.as_ref() {
            yr = rat(y);
        } else if m.x_res.is_some() {
            // If y missing but x exists, mirror x
            yr = Rational { n: xr.n, d: xr.d };
        }

        // If x missing but y exists, mirror y
        if m.x_res.is_none() && m.y_res.is_some() {
            xr = Rational { n: yr.n, d: yr.d };
        }
    }

    // Avoid nonsense denom=0
    if xr.d == 0 {
        xr = Rational { n: 600, d: 1 };
    }
    if yr.d == 0 {
        yr = Rational { n: 600, d: 1 };
    }

    (unit, xr, yr)
}

fn write_tiff_rgb16(
    out_path: &Path,
    w: u32,
    h: u32,
    pix: &[RGB16],
    icc: Option<&[u8]>,
    meta: Option<&TiffMeta>,
) -> Result<()> {
    let f = File::create(out_path).with_context(|| format!("Create output: {}", out_path.display()))?;
    let mut tiff = TiffEncoder::new(BufWriter::new(f))?;

    let mut img = tiff.new_image::<colortype::RGB16>(w, h)?;

    // Resolution tags
    let (unit, xr, yr) = normalize_resolution(meta);
    img.resolution_unit(unit);
    img.x_resolution(xr);
    img.y_resolution(yr);

    // Embed ICC into TIFF (tag 34675) as UNDEFINED (7)
    if let Some(icc_bytes) = icc {
        img.encoder()
            .write_tag(Tag::Unknown(34675), UndefinedBytes(icc_bytes))
            .context("Write ICCProfile tag (34675) as UNDEFINED")?;
    }

    // Stream write by strips (avoid huge raw allocation)
    img.rows_per_strip(64)?;

    let mut row = 0u32;
    while img.next_strip_sample_count() > 0 {
        let rows = min(64, h - row);
        let start = (row as usize) * (w as usize);
        let end = ((row + rows) as usize) * (w as usize);
        let slice = &pix[start..end];

        let mut raw: Vec<u16> = Vec::with_capacity(slice.len() * 3);
        for p in slice {
            raw.push(p.r);
            raw.push(p.g);
            raw.push(p.b);
        }

        img.write_strip(&raw)?;
        row += rows;
    }

    img.finish()?;
    Ok(())
}

fn write_tiff_rgb8(
    out_path: &Path,
    w: u32,
    h: u32,
    pix: &[RGB8],
    icc: Option<&[u8]>,
    meta: Option<&TiffMeta>,
) -> Result<()> {
    let f = File::create(out_path).with_context(|| format!("Create output: {}", out_path.display()))?;
    let mut tiff = TiffEncoder::new(BufWriter::new(f))?;

    let mut img = tiff.new_image::<colortype::RGB8>(w, h)?;

    let (unit, xr, yr) = normalize_resolution(meta);
    img.resolution_unit(unit);
    img.x_resolution(xr);
    img.y_resolution(yr);

    // Embed ICC into TIFF (tag 34675) as UNDEFINED (7)
    if let Some(icc_bytes) = icc {
        img.encoder()
            .write_tag(Tag::Unknown(34675), UndefinedBytes(icc_bytes))
            .context("Write ICCProfile tag (34675) as UNDEFINED")?;
    }

    img.rows_per_strip(128)?;

    let mut row = 0u32;
    while img.next_strip_sample_count() > 0 {
        let rows = min(128, h - row);
        let start = (row as usize) * (w as usize);
        let end = ((row + rows) as usize) * (w as usize);
        let slice = &pix[start..end];

        let mut raw: Vec<u8> = Vec::with_capacity(slice.len() * 3);
        for p in slice {
            raw.push(p.r);
            raw.push(p.g);
            raw.push(p.b);
        }

        img.write_strip(&raw)?;
        row += rows;
    }

    img.finish()?;
    Ok(())
}

// ---------------- File operations ----------------

fn is_supported_image_ext(p: &Path) -> bool {
    match p
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
    {
        Some(ext) => matches!(ext.as_str(), "tif" | "tiff" | "png" | "jpg" | "jpeg"),
        None => false,
    }
}

fn collect_input_files(root: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if recursive {
        for e in WalkDir::new(root).follow_links(false) {
            let e = e?;
            if e.file_type().is_file() {
                let p = e.path();
                if is_supported_image_ext(p) {
                    files.push(p.to_path_buf());
                }
            }
        }
    } else {
        for e in std::fs::read_dir(root)? {
            let e = e?;
            let p = e.path();
            if p.is_file() && is_supported_image_ext(&p) {
                files.push(p);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn normalize_out_ext(ext: &str) -> Result<String> {
    let e = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    if matches!(e.as_str(), "tif" | "tiff" | "png" | "jpg" | "jpeg") {
        Ok(e)
    } else {
        bail!("Unsupported --out-ext: {ext}. Use one of: tif, tiff, png, jpg, jpeg");
    }
}

fn sidecar_path_for(output: &Path) -> PathBuf {
    let mut p = output.to_path_buf();
    p.set_extension("icc");
    p
}

fn convert_one(args: &Args, eff: &Effective, input: &Path, output: &Path) -> Result<()> {
    let in_is_tiff = is_tiff_path(input);
    let out_is_tiff = is_tiff_path(output);

    // Read TIFF meta (ICC + resolution) cheaply if input is TIFF.
    let tiff_meta = if in_is_tiff {
        match read_tiff_meta(input) {
            Ok(meta) => Some(meta),
            Err(e) => {
                eprintln!(
                    "Warning: could not read TIFF metadata from {}: {}",
                    input.display(),
                    e
                );
                None
            }
        }
    } else {
        None
    };

    let in_prof = pick_input_profile(args, tiff_meta.as_ref())
        .with_context(|| format!("Pick input ICC profile for {}", input.display()))?;

    // Input ICC bytes (for "preserve embedded ICC" behavior)
    let in_icc_bytes = tiff_meta.as_ref().and_then(|m| m.icc.as_deref());

    let out_prof_opt = pick_output_profile_with_policy(args, eff.preset, &in_prof, in_icc_bytes)
        .with_context(|| format!("Pick output ICC profile (policy) for {}", input.display()))?;

    // Optional: write ICC sidecar next to each output image
    if args.write_icc {
        if let Some(out_prof) = out_prof_opt.as_ref() {
            match out_prof.icc() {
                Ok(out_bytes) => {
                    let path = sidecar_path_for(output);
                    fs::write(&path, out_bytes)
                        .with_context(|| format!("Write ICC sidecar to {}", path.display()))?;
                }
                Err(e) => {
                    eprintln!(
                        "Warning: could not export ICC for sidecar {}: {}",
                        output.display(),
                        e
                    );
                }
            }
        }
    }

    // Debug info - print for each file
    if args.debug_icc {
        match in_prof.icc() {
            Ok(in_bytes) => {
                eprintln!(
                    "[icc] {} -> in_profile: {} bytes (v{:.4})",
                    input.display(),
                    in_bytes.len(),
                    in_prof.version()
                );
            }
            Err(e) => {
                eprintln!(
                    "[icc] {} -> failed to get input ICC: {}",
                    input.display(),
                    e
                );
            }
        }

        if let Some(out_prof) = out_prof_opt.as_ref() {
            match out_prof.icc() {
                Ok(out_bytes) => {
                    eprintln!(
                        "[icc] {} -> out_profile: {} bytes (v{:.4})",
                        output.display(),
                        out_bytes.len(),
                        out_prof.version()
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[icc] {} -> failed to get output ICC: {}",
                        output.display(),
                        e
                    );
                }
            }
        } else {
            eprintln!(
                "[icc] {} -> out_profile: (NONE) per policy",
                output.display()
            );
        }
    }

    // Load image pixels (RGB16 path; we quantize later if needed)
    let (w, h, mut rgb16) = load_rgb16(input)
        .with_context(|| format!("Load image as RGB16 from {}", input.display()))?;

    // If no ICC transform requested or policy disables ICC output: just depth conversion.
    if args.no_icc || out_prof_opt.is_none() {
        match eff.out_depth {
            BitDepth::B16 => {
                if out_is_tiff {
                    write_tiff_rgb16(output, w, h, &rgb16, None, tiff_meta.as_ref())
                        .with_context(|| format!("Write TIFF RGB16 to {}", output.display()))?;
                } else {
                    let mut raw = Vec::<u16>::with_capacity(rgb16.len() * 3);
                    for p in &rgb16 {
                        raw.push(p.r);
                        raw.push(p.g);
                        raw.push(p.b);
                    }
                    let buf = image::ImageBuffer::<image::Rgb<u16>, Vec<u16>>::from_raw(w, h, raw)
                        .context("Create RGB16 buffer")?;
                    buf.save(output)
                        .with_context(|| format!("Save image to {}", output.display()))?;
                }
            }
            BitDepth::B8 => {
                let rgb8 = quantize_rgb16_to_rgb8_stream_dither(&rgb16, w, h, eff.tone_map, eff.dither);
                if out_is_tiff {
                    write_tiff_rgb8(output, w, h, &rgb8, None, tiff_meta.as_ref())
                        .with_context(|| format!("Write TIFF RGB8 to {}", output.display()))?;
                } else {
                    let mut raw = Vec::<u8>::with_capacity(rgb8.len() * 3);
                    for p in &rgb8 {
                        raw.push(p.r);
                        raw.push(p.g);
                        raw.push(p.b);
                    }
                    let buf = image::RgbImage::from_raw(w, h, raw).context("Create RGB8 buffer")?;
                    buf.save(output)
                        .with_context(|| format!("Save image to {}", output.display()))?;
                }
            }
        }
        return Ok(());
    }

    let out_prof = out_prof_opt.expect("checked above");
    let intent: Intent = eff.intent.into();

    let mut flags = Flags::default();
    if eff.bpc {
        flags = flags | Flags::BLACKPOINT_COMPENSATION;
    }

    // Transform in 16-bit
    let xform: Transform<RGB16, RGB16> = Transform::new_flags(
        &in_prof,
        PixelFormat::RGB_16,
        &out_prof,
        PixelFormat::RGB_16,
        intent,
        flags,
    )?;
    xform.transform_in_place(&mut rgb16);

    // Decide ICC embedding bytes for TIFF outputs (MC and UC-II end up here).
    let embed_icc_bytes = if out_is_tiff {
        match out_prof.icc() {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                eprintln!(
                    "Warning: could not export output ICC bytes for {}: {}",
                    output.display(),
                    e
                );
                None
            }
        }
    } else {
        None
    };

    match eff.out_depth {
        BitDepth::B16 => {
            if out_is_tiff {
                write_tiff_rgb16(
                    output,
                    w,
                    h,
                    &rgb16,
                    embed_icc_bytes.as_deref(),
                    tiff_meta.as_ref(),
                )
                .with_context(|| {
                    format!("Write TIFF RGB16 with ICC to {}", output.display())
                })?;
            } else {
                let mut raw = Vec::<u16>::with_capacity(rgb16.len() * 3);
                for p in &rgb16 {
                    raw.push(p.r);
                    raw.push(p.g);
                    raw.push(p.b);
                }
                let buf = image::ImageBuffer::<image::Rgb<u16>, Vec<u16>>::from_raw(w, h, raw)
                    .context("Create RGB16 buffer")?;
                buf.save(output)
                    .with_context(|| format!("Save image to {}", output.display()))?;
            }
        }
        BitDepth::B8 => {
            let rgb8 = quantize_rgb16_to_rgb8_stream_dither(&rgb16, w, h, eff.tone_map, eff.dither);
            if out_is_tiff {
                write_tiff_rgb8(
                    output,
                    w,
                    h,
                    &rgb8,
                    embed_icc_bytes.as_deref(),
                    tiff_meta.as_ref(),
                )
                .with_context(|| format!("Write TIFF RGB8 with ICC to {}", output.display()))?;
            } else {
                let mut raw = Vec::<u8>::with_capacity(rgb8.len() * 3);
                for p in &rgb8 {
                    raw.push(p.r);
                    raw.push(p.g);
                    raw.push(p.b);
                }
                let buf = image::RgbImage::from_raw(w, h, raw).context("Create RGB8 buffer")?;
                buf.save(output)
                    .with_context(|| format!("Save image to {}", output.display()))?;
            }
        }
    }

    Ok(())
}

fn process_batch_conversion(
    args: &Args,
    eff: &Effective,
    in_dir: &Path,
    out_dir: &Path,
    out_ext: &str,
    inputs: Vec<PathBuf>,
    jobs: Option<usize>,
) -> Result<()> {
    // Funkce pro zpracování jednoho souboru v batch režimu
    let process_single = |input_path: &Path| -> Result<()> {
        let rel = match input_path.strip_prefix(in_dir) {
            Ok(r) => r,
            Err(_) => {
                return Err(anyhow!(
                    "Failed to compute relative path for {}",
                    input_path.display()
                ));
            }
        };

        let rel_parent = rel.parent().unwrap_or(Path::new(""));
        let stem = match input_path.file_stem() {
            Some(s) => s,
            None => {
                return Err(anyhow!(
                    "Invalid file name (no stem): {}",
                    input_path.display()
                ));
            }
        };

        let stem_str = match stem.to_str() {
            Some(s) => s,
            None => {
                return Err(anyhow!(
                    "Invalid UTF-8 file name: {}",
                    input_path.display()
                ));
            }
        };

        let target_dir = out_dir.join(rel_parent);
        if let Err(e) = std::fs::create_dir_all(&target_dir) {
            return Err(anyhow!(
                "Failed to create directory {}: {}",
                target_dir.display(),
                e
            ));
        }

        let out_name = format!("{}{}.{}", stem_str, args.suffix, out_ext);
        let out_path = target_dir.join(out_name);

        if out_path.exists() && !args.overwrite {
            eprintln!("Skipping existing: {}", out_path.display());
            return Ok(());
        }

        convert_one(args, eff, input_path, &out_path)
            .map_err(|e| anyhow!("{} -> {}: {}", input_path.display(), out_path.display(), e))
    };

    // Paralelní zpracování
    let run = || -> Result<()> {
        let pool = match jobs {
            Some(n) => {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(n)
                    .build()
                    .context("Failed to create thread pool")?
            }
            None => rayon::ThreadPoolBuilder::new()
                .build()
                .context("Failed to create thread pool")?,
        };

        pool.install(|| {
            let results: Vec<Result<()>> = inputs.par_iter().map(|input_path| process_single(input_path)).collect();

            // Aggregate errors (if any)
            let mut ok = 0usize;
            let mut errs = Vec::new();
            for r in results {
                match r {
                    Ok(()) => ok += 1,
                    Err(e) => errs.push(e),
                }
            }

            if !errs.is_empty() {
                eprintln!("Completed with errors: ok={ok}, errors={}", errs.len());
                for e in errs.iter().take(20) {
                    eprintln!("  - {e}");
                }
                if errs.len() > 20 {
                    eprintln!("  ... and {} more errors", errs.len() - 20);
                }
                bail!("Batch conversion failed with {} errors.", errs.len());
            }

            eprintln!("Batch conversion successful: {ok} files processed.");
            Ok(())
        })
    };

    run()
}

fn main() -> Result<()> {
    let args = Args::parse();
    let eff = compute_effective(&args);

    if args.input.is_dir() {
        let in_dir = &args.input;
        let out_dir = &args.output;

        if out_dir.exists() && !out_dir.is_dir() {
            bail!(
                "OUTPUT must be a directory when INPUT is a directory: {}",
                out_dir.display()
            );
        }

        if let Err(e) = std::fs::create_dir_all(out_dir) {
            bail!(
                "Failed to create output directory {}: {}",
                out_dir.display(),
                e
            );
        }

        let out_ext = normalize_out_ext(&args.out_ext)?;
        let inputs = collect_input_files(in_dir, args.recursive)?;

        if inputs.is_empty() {
            bail!("No supported images found in {}", in_dir.display());
        }

        eprintln!("Found {} files to process", inputs.len());

        let jobs = if args.jobs == 0 { None } else { Some(args.jobs) };

        process_batch_conversion(&args, &eff, in_dir, out_dir, &out_ext, inputs, jobs)?;
    } else {
        // Single-file mode
        if args.output.is_dir() {
            bail!(
                "OUTPUT must be a file when INPUT is a file: {}",
                args.output.display()
            );
        }

        if args.output.exists() && !args.overwrite {
            bail!(
                "Output file already exists: {}. Use --overwrite to replace.",
                args.output.display()
            );
        }

        convert_one(&args, &eff, &args.input, &args.output).with_context(|| {
            format!(
                "Failed to convert {} to {}",
                args.input.display(),
                args.output.display()
            )
        })?;
    }

    Ok(())
}
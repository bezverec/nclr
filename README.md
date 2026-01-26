# NCLR: NDK Color Conversion Tool

**ICC-aware color-space and bit-depth conversion for digitization workflows**

NCLR provides a **reference preprocessing step** for digitization pipelines where archival-quality images must be converted into viewer-compatible derivatives **before** JPEG 2000 compression.

It is designed for use in memory institutions (libraries, archives, museums) and aligns with NDK-style digitization practice.

---

## Why this tool exists

JPEG 2000 encoders (Kakadu, Grok, OpenJPEG):

- compress image data,
- but **do not perform controlled ICC color conversion**, and
- **do not define deterministic 16→8 bit-depth mapping**.

Relying on JPEG 2000 encoders for these tasks leads to:

- undefined or implementation-specific behavior,
- loss of auditability,
- inconsistent visual results.

**Conclusion:**  
Color-space conversion and bit-depth reduction must be performed *before* JPEG 2000 compression, using a proper Color Management System (CMS).

NCLR implements that step using **LittleCMS 2** via the safe Rust crate `lcms2`.

> Note: JPEG input is supported but typically already 8-bit and sRGB.  
> NCLR is most useful with TIFF/PNG sources.

---

## Core features

- ICC-aware color conversion (LittleCMS 2)
- Embedded ICC detection:
  - TIFF (tag **34675** – ICCProfile)
  - JPEG (APP2 `ICC_PROFILE` segments)
- Configurable rendering intents:
  - Perceptual
  - Relative Colorimetric
  - Absolute Colorimetric
  - Saturation
- Optional Black Point Compensation (BPC)
- High-quality 16-bit → 8-bit conversion:
  - ICC transform performed in 16-bit precision
  - optional tone mapping
  - optional Floyd–Steinberg dithering
- Deterministic, reproducible output
- For TIFF output:
  - embeds output ICC (when policy allows it)
  - writes proper resolution tags (XResolution, YResolution, ResolutionUnit)
- Batch conversion:
  - input **file or directory**
  - output **file or directory**
  - optional `--recursive / -r`
  - parallel processing (`--jobs`)

---

## Typical NDK-style workflow

### Master Copy (MC)

- 16 bits per channel
- RGB working space (e.g. **eciRGBv2**, AdobeRGB1998, scanner/workspace RGB)
- Embedded ICC profile
- Lossless JPEG 2000 (5-3 reversible)

### User Copy (UC)

- 8 bits per channel
- sRGB color space (explicitly declared for UC-II)
- Lossy JPEG 2000 (9-7 irreversible)
- Optimized for web / IIIF viewers

**This tool converts MC → UC input images.  
JPEG 2000 compression happens afterwards.**

---

## Important: MC → MC rewrite for JHOVE-valid TIFF ("MC II")

In normal NDK digitization workflows you typically **do not** need to convert **Master Copy → Master Copy**.

However, real-world digitization collections often contain **source "MC" TIFFs** with non-standard scanner/vendor tags or malformed IFD entries. Preservation validation tools (e.g. **JHOVE TIFF-hul**) may report errors such as:

- *Type mismatch for tag …; expecting 7, saw 2*
- other metadata/IFD inconsistencies

These files may still be viewable, but they are **not well-formed** under strict validation.

### Recommended practice: MC → MC rewrite ("MC II")

If your goal is a **JHOVE-valid TIFF**, NCLR can be used to **rewrite the TIFF as MC again**. This produces a normalized archival master, recommended to label as:

**MC II** = *NDK-normalized Master Copy rewrite*  
("second-generation MC" created intentionally for NDK compliance and validation)

What NCLR guarantees in MC → MC mode:

- pixel data preserved in **16-bit**
- ICC profile embedded correctly (TIFF tag **34675 / ICCProfile**)
- resolution tags written properly
- a clean, standard TIFF structure suitable for validation workflows

What is intentionally **not** preserved:

- EXIF/XMP blocks
- scanner/vendor/private TIFF tags
- other nonessential metadata

> This metadata loss is intentional and aligns with NDK-style preservation practice: the preservation master is defined primarily by **pixel content + ICC + resolution + essential TIFF structure**.

### When to use MC → MC ("MC II")

Use this when:

- the source TIFF is **not JHOVE-valid** (type mismatch, malformed tags),
- you need a **clean archival baseline** before JPEG2000 or long-term storage,
- you accept dropping EXIF/XMP/vendor tags as part of NDK normalization.

### When NOT to use MC → MC

Avoid MC → MC rewrite when:

- you must preserve scanner provenance metadata (EXIF/XMP/vendor tags),
- you rely on proprietary tags downstream.

If you must keep metadata, handle it in a separate pipeline stage (e.g. tag copying with ExifTool after conversion). Note that invalid tag typing cannot always be "fixed" without rewriting the TIFF structure.

---

# NCLR Usage Examples

**Notes:**
- Prefer `--preset` for NDK-style workflows. You can still override anything explicitly.
- For 16-bit workflows use TIFF/PNG as input (JPEG is typically 8-bit).
- Under NDK policy:
  - `UC-I` forces **NO output ICC** (unless `--force-out-icc`).
  - `UC-II` embeds **sRGB ICC into the output TIFF** by default (unless overridden via `--out-icc`).
  - `MC` preserves the **embedded input ICC** by default (no forced sRGB unless you set `--out-icc`).

---

## 1) Recommended Way: Presets

### NDK UC-II (maps/manuscripts/old prints): 16-bit MC → 8-bit UC, output ICC (sRGB embedded)

```bash
nclr \
  --preset ndk-uc-ii \
  --input MC_16bit.tif \
  --output UC_8bit.tif
```

### NDK UC-I (books/periodicals): 16-bit MC → 8-bit UC, **no output ICC** (policy)

```bash
nclr \
  --preset ndk-uc-i \
  --input MC_16bit.tif \
  --output UC_8bit.tif
```

### NDK MC → MC rewrite ("MC II"): 16-bit output, ICC preserved (use when you need JHOVE-valid TIFF)

```bash
nclr \
  --preset ndk-mc \
  --input MC_source.tif \
  --output MC_II_normalized.tif
```

---

## 2) Batch Conversion (Directory → Directory)

Convert all supported images in a folder to TIFF, keeping base names:

```bash
nclr \
  --preset ndk-uc-ii \
  --input  D:\scans\MC \
  --output D:\scans\UC \
  --out-ext tif
```

Recursive variant (also available as `-r`):

```bash
nclr --preset ndk-uc-ii -r --input D:\scans\MC --output D:\scans\UC --out-ext tif
```

Parallel processing (0 = default Rayon behavior, otherwise set explicit worker count):

```bash
nclr --preset ndk-uc-ii -r --jobs 8 --input D:\scans\MC --output D:\scans\UC
```

---

## 3) Explicit NDK Profile (Same Idea as Presets)

### UC-II via policy flag

```bash
nclr \
  --ndk-profile uc-ii \
  --input MC_16bit.tif \
  --output UC_8bit.tif
```

### UC-I via policy flag (forces ICC OFF)

```bash
nclr \
  --ndk-profile uc-i \
  --input MC_16bit.tif \
  --output UC_8bit.tif
```

### MC via policy flag (MC → MC rewrite)

```bash
nclr \
  --ndk-profile mc \
  --input MC_source.tif \
  --output MC_II_normalized.tif
```

---

## 4) ICC Handling Examples

### Automatic embedded ICC detection (default `--detect-input-icc auto`)

```bash
nclr \
  --preset ndk-uc-ii \
  --input MC_16bit.tif \
  --output UC_8bit.tif \
  --detect-input-icc auto
```

### Force input ICC to sRGB (ignore embedded profile)

```bash
nclr \
  --preset ndk-uc-ii \
  --input input.tif \
  --output out.tif \
  --detect-input-icc srgb
```

### Provide explicit input ICC file (scanner/workspace profile)

```bash
nclr \
  --preset ndk-uc-ii \
  --input input.tif \
  --output out.tif \
  --detect-input-icc file \
  --input-icc-file scanner_profile.icc
```

### UC-I but force output ICC anyway (override policy)

```bash
nclr \
  --ndk-profile uc-i \
  --force-out-icc \
  --out-icc sRGB.icc \
  --input input.tif \
  --output out.tif
```

### Write ICC sidecar(s) (next to output image)

Single-file example (writes `out.icc` next to `out.tif`):

```bash
nclr --preset ndk-uc-ii --write-icc --input input.tif --output out.tif
```

Batch example (writes `*.icc` next to each output image in the output directory):

```bash
nclr --preset ndk-uc-ii --write-icc --input D:\scans\MC --output D:\scans\UC --out-ext tif
```

---

## 5) Rendering Intent + BPC

### Perceptual (good default for viewer derivatives)

```bash
nclr \
  --preset ndk-uc-ii \
  --intent perceptual \
  --bpc true \
  --input input.tif \
  --output out.tif
```

### Relative colorimetric + BPC (often preferred for "faithful" reproduction)

```bash
nclr \
  --preset ndk-uc-ii \
  --intent relative \
  --bpc true \
  --input input.tif \
  --output out.tif
```

### Absolute colorimetric (proofing-type workflows)

```bash
nclr \
  --preset ndk-uc-ii \
  --intent absolute \
  --bpc false \
  --input input.tif \
  --output out.tif
```

---

## 6) Bit Depth, Tone Mapping and Dithering

### Force 8-bit output explicitly

```bash
nclr \
  --preset ndk-uc-ii \
  --out-depth b8 \
  --input input.tif \
  --output out.tif
```

### 16→8 with tone mapping (Gamma) + dithering (helps gradients)

```bash
nclr \
  --preset ndk-uc-ii \
  --tone-map gamma \
  --dither true \
  --input input.tif \
  --output out.tif
```

### 16→8 with perceptual tone mapping + dithering

```bash
nclr \
  --preset ndk-uc-ii \
  --tone-map perceptual \
  --dither true \
  --input input.tif \
  --output out.tif
```

---

## 7) Diagnostics / Troubleshooting

### Print ICC diagnostics (profile sizes + versions)

```bash
nclr \
  --preset ndk-uc-ii \
  --debug-icc \
  --input input.tif \
  --output out.tif
```

### Skip ICC transform entirely (only bit depth conversion / alpha drop)

```bash
nclr \
  --no-icc \
  --out-depth b8 \
  --input input.tif \
  --output out.tif
```

---

## 8) Pipeline Example: NCLR → JPEG2000 (Grok)

### UC-II (NDK-ish): convert to 8-bit sRGB first, then compress to JP2

```bash
nclr \
  --preset ndk-uc-ii \
  --input MC_16bit.tif \
  --output UC_8bit_srgb.tif

grk_compress -i UC_8bit_srgb.tif -o UC.jp2 \
  -r "362,256,181,128,90,64,45,32,22,16,11,8" \
  -I -t 1024,1024 -p RPCL -n 6 \
  -c [256,256],[256,256],[128,128],[128,128],[128,128],[128,128] \
  -b 64,64 -X -M 1 -u R -f -H 4
```

---

## Key Batch Conversion Options Summary

| Option | Description | Example |
|--------|-------------|---------|
| `-i`, `--input` | Input file or directory | `--input D:\scans\MC` |
| `-o`, `--output` | Output file or directory | `--output D:\scans\UC` |
| `-r`, `--recursive` | Scan subdirectories | `-r` |
| `--out-ext` | Output file extension (default: `tif`) | `--out-ext tif` |
| `--suffix` | Append suffix to output filenames | `--suffix "_uc"` |
| `--overwrite` | Replace existing files | `--overwrite` |
| `--jobs` | Number of parallel workers (0=auto) | `--jobs 4` |
| `--write-icc` | Create `.icc` sidecar files | `--write-icc` |

All examples are ready to copy-paste and work with the current NCLR implementation. The `--write-icc` flag automatically creates sidecar files with the same base name as the output file but with `.icc` extension.

---

## Rendering intents explained

| Intent                | Description                                                  | Typical use           |
| --------------------- | ------------------------------------------------------------ | --------------------- |
| Perceptual            | Preserves overall visual appearance by compressing the gamut | Default for User Copy |
| Relative Colorimetric | Preserves in-gamut colors, clips out-of-gamut colors         | Faithful reproduction |
| Absolute Colorimetric | Preserves absolute color values incl. white point            | Proofing              |
| Saturation            | Prioritizes color vividness                                  | Charts, graphics      |

---

## Black Point Compensation (BPC)

Black Point Compensation adjusts shadow mapping between source and destination profiles with different black points.

**Recommendations:**

Enable BPC for:

- Perceptual intent
- Relative Colorimetric intent

Disable BPC only for specialized proofing scenarios.

---

## Bit-depth conversion strategy

When converting 16-bit → 8-bit output:

1.  Decode input image to 16-bit RGB
2.  Apply ICC transform in 16-bit precision
3.  Optionally apply tone mapping
4.  Optionally apply dithering
5.  Quantize to 8-bit RGB

This ensures maximum color accuracy and avoids precision loss during ICC mapping.

---

## What this tool does NOT do

- JPEG 2000 compression
- Metadata preservation/transfer (EXIF/XMP and vendor TIFF tags may be dropped)
- Image resizing or tiling

NCLR writes a new image file and guarantees:

- pixel data (after conversion)
- ICC embedding (TIFF tag 34675) when policy allows it
- TIFF resolution tags (XResolution/YResolution/ResolutionUnit)

Everything else should be handled by dedicated tools in subsequent pipeline stages.

---

## Technical foundation

- **Color Management:** LittleCMS 2
- **Rust API:** `lcms2` crate (safe wrapper)
- **Image I/O:** `image` crate
- **ICC standards:** ICC v2 / v4 compliant

---

## Intended audience

- Digitization engineers
- Digital preservation specialists
- Library / archive infrastructure teams
- IIIF / image server operators

---

## Command-line interface (CLI reference)

The tool follows a **policy + override** design:

- `--preset` provides **high-level recommended defaults** for common NDK workflows
- explicit options **always override** preset values

---

### Synopsis

```bash
nclr [OPTIONS] --input <INPUT> --output <OUTPUT>
```

---

## Presets (recommended)

### `--preset <PRESET>`

High-level convenience presets that fill in **recommended defaults** according to NDK-style digitization practice.

Explicit options you pass (e.g. `--intent`, `--out-depth`, `--tone-map`, `--dither`) **always take precedence** over the preset.

Example:

```bash
nclr --preset ndk-uc-ii -i MC.tif -o UC.tif
```

| Preset        | Intended use | Policy summary |
|---------------|--------------|----------------|
| `ndk-mc`      | Master Copy rewrite ("MC II") | 16-bit output, ICC enabled (preserve input ICC) |
| `ndk-uc-i`    | User Copy I (books, periodicals) | 8-bit output, **ICC disabled** |
| `ndk-uc-ii`   | User Copy II (maps, manuscripts, old prints) | 8-bit output, ICC enabled (sRGB default) |

> NDK reference: *[Standardy pro obrazová data](https://standardy.ndk.cz/ndk/standardy-digitalizace/standardy-pro-obrazova-data)*

---

## Input / output

### `-i, --input <PATH>`

Input image.  
Supported formats: TIFF, PNG, JPEG.

> For 16-bit workflows, use **TIFF or PNG**.

### `-o, --output <PATH>`

Output image path.  
Format is inferred from file extension.

### Batch conversion options

When `--input` is a directory, the following options apply:

#### `-r, --recursive`

Scan subdirectories for images.

#### `--out-ext <EXTENSION>`

Output file extension (default: `tif`).  
Supported: `tif`, `tiff`, `png`, `jpg`, `jpeg`.

#### `--suffix <SUFFIX>`

Append suffix to output filename stem.  
Example: `--suffix "_uc"` converts `image.tif` → `image_uc.tif`.

#### `--overwrite`

Replace existing output files.

#### `--jobs <NUMBER>`

Number of parallel workers (0 = auto).  
Example: `--jobs 4` uses 4 CPU cores.

---

## ICC handling

### `--detect-input-icc <auto|srgb|file>`

How to determine the **input ICC profile**.

| Mode | Behavior |
|-----|----------|
| `auto` | Use embedded ICC if present, otherwise fallback to sRGB |
| `srgb` | Force sRGB, ignore embedded profiles |
| `file` | Load ICC from `--input-icc-file` |

Default: `auto`

Notes:

- TIFF: embedded ICC is read from TIFF tag **34675** (ICCProfile).
- JPEG: embedded ICC is read from APP2 `ICC_PROFILE` segments.
- If `auto` falls back to sRGB, it is an **assumption** (use `file` for known scanner/workspace profiles).

---

### `--input-icc-file <PATH>`

ICC profile used when `--detect-input-icc=file`.

---

### `--out-icc <PATH>`

Explicit output ICC profile.

NDK policy interaction:

- **UC-I**: ignored unless `--force-out-icc` is set
- **UC-II**: defaults to sRGB if not specified
- **MC**: if not specified, **preserves embedded input ICC** (no forced sRGB)

---

### `--force-out-icc`

Overrides NDK policy and allows output ICC for **UC-I**.

---

### `--write-icc`

Write output ICC profile to a **sidecar file** next to each output image.

The sidecar path is derived from the output image path by changing the extension to `.icc`.

Examples:

```bash
# Single file: creates out.icc next to out.tif
nclr --preset ndk-uc-ii --write-icc --input input.tif --output out.tif

# Batch: creates *.icc next to each output file
nclr --preset ndk-uc-ii --write-icc --input D:\scans\MC --output D:\scans\UC --out-ext tif
```

> Note: for TIFF output, the ICC profile is embedded directly into the TIFF when policy allows it.  
> Sidecar is only written when you explicitly request it.

---

### `--debug-icc`

Print ICC diagnostics:

- profile byte size
- ICC version

---

## Color conversion

### `--intent <perceptual|relative|absolute|saturation>`

Rendering intent used for ICC transform.

| Intent | Description | Typical use |
|------|-------------|-------------|
| perceptual | Compresses gamut, preserves visual appearance | Default for UC |
| relative | Preserves in-gamut colors, clips out-of-gamut | Faithful reproduction |
| absolute | Preserves absolute color values incl. white point | Proofing |
| saturation | Prioritizes vivid colors | Charts, graphics |

---

### `--bpc [true|false]`

Black Point Compensation.

Default: `true`

Recommended for:

- perceptual intent
- relative colorimetric intent

---

## Bit depth and quantization

### `--out-depth <b8|b16>`

Output bit depth.

NDK defaults:

- MC → `b16`
- UC-I / UC-II → `b8`

---

### `--tone-map <none|gamma|perceptual>`

Tone mapping applied **after ICC transform** when converting 16→8.

| Mode | Description |
|-----|-------------|
| none | Linear mapping |
| gamma | Gamma 2.2 |
| perceptual | Square-root curve |

---

### `--dither <true|false>`

Apply Floyd–Steinberg dithering after 16→8 quantization.

Recommended for:

- smooth gradients
- maps and illustrations

---

## Special modes

### `--no-icc`

Skip ICC transform entirely.

Only performs:

- bit depth conversion
- alpha channel removal

Useful for debugging or special workflows.

---

### `-h, --help`

Print help summary.

### `-V, --version`

Print version.
---

## What exactly does each preset set?

Presets provide **recommended default values** for common NDK workflows.  
They do **not lock** any option — **explicit CLI arguments always override preset values**.

This section documents **exactly which options are filled by each preset**.

---

### `--preset ndk-mc`
**NDK Master Copy (archival copy rewrite / "MC-II")**

Use this when you need a *clean, standards-friendly* TIFF rewrite (e.g., for **JHOVE-valid** output TIFF).  
NCLR preserves **pixel data + ICC (tag 34675) + resolution tags**, but may drop **non-core TIFF metadata** (EXIF/XMP/vendor/private tags) by design. Treat the rewritten result as **MC-II** under NDK practice.

| Option | Value | Meaning |
|------|------|--------|
| `--ndk-profile` | `mc` | Archival policy |
| `--out-depth` | `b16` | Preserve full precision |
| `--detect-input-icc` | `auto` | Use embedded ICC if present |
| `--out-icc` | *(not set)* | Preserve embedded input ICC (no normalization target) |
| `--intent` | `perceptual` | Used only if an actual profile conversion happens |
| `--bpc` | `true` | Safe default |
| `--tone-map` | `none` | No tonal alteration |
| `--dither` | `false` | Never dither archival data |
| `--write-icc` | *(not set)* | No sidecar by default |

**Effective colorspace:**  
→ **same as input ICC** (if embedded; e.g. eciRGBv2)  
→ output TIFF contains ICC (tag 34675) under MC policy

---

### `--preset ndk-uc-i`
**NDK User Copy I (books, periodicals)**

| Option | Value | Meaning |
|------|------|--------|
| `--ndk-profile` | `uc-i` | User Copy I policy |
| `--out-depth` | `b8` | Viewer compatibility |
| `--detect-input-icc` | `auto` | ICC may be detected, but output ICC is disabled by policy |
| `--out-icc` | **disabled** | No output ICC (NDK requirement) |
| `--intent` | `perceptual` | Relevant only if you override policy with `--force-out-icc` |
| `--bpc` | `true` | Safe default |
| `--tone-map` | `none` | Avoid noise / text distortion |
| `--dither` | `false` | Avoid grain in paper background |
| `--write-icc` | *(not set)* | No ICC sidecar |

**Effective colorspace:**  
→ **implicit RGB** (no declared ICC)  
→ viewer decides interpretation (NDK-compliant)

**Policy note:** By default, UC-I disables output ICC, so NCLR also skips the ICC transform.  
If you want a controlled transform (e.g., normalize to sRGB) while keeping UC-I intent, use `--force-out-icc`.

---

### `--preset ndk-uc-ii`
**NDK User Copy II (maps, manuscripts, old prints)**

| Option | Value | Meaning |
|------|------|--------|
| `--ndk-profile` | `uc-ii` | User Copy II policy |
| `--out-depth` | `b8` | Viewer compatibility |
| `--detect-input-icc` | `auto` | Use embedded ICC if present |
| `--out-icc` | `sRGB` | Explicit normalization target |
| `--intent` | `perceptual` | Best visual appearance |
| `--bpc` | `true` | Preserves shadow detail |
| `--tone-map` | `perceptual` | Reduce banding risk |
| `--dither` | `true` | Improve gradients |
| `--write-icc` | *(not set)* | No sidecar by default (use `--write-icc` explicitly) |

**Effective colorspace:**  
→ **sRGB (explicitly declared)**  
→ maximum compatibility with IIIF / browsers

---

## Implicit defaults (no preset)

If no `--preset` is used, the tool defaults to conservative settings:

- tone mapping: `none`
- dithering: `false`

Note: `--ndk-profile` still has a default (`uc-ii`), so policy-derived defaults still apply.

---

## Defaults when **no `--preset`** is used

This section documents the tool’s **implicit defaults** (behavior when you do **not** pass `--preset`).

### 1) Global defaults (apply unless explicitly overridden)

| Option | Default | What it means |
|---|---:|---|
| `--detect-input-icc` | `auto` | Use embedded ICC if present (TIFF/JPEG), otherwise assume **sRGB** |
| `--intent` | `perceptual` | Rendering intent for ICC transform (when active) |
| `--bpc` | `true` | Black Point Compensation enabled |
| `--tone-map` | `none` | No tone curve applied during 16→8 conversion |
| `--dither` | `false` | No Floyd–Steinberg dithering |
| `--no-icc` | `false` | ICC transform enabled (unless policy disables output ICC) |
| `--force-out-icc` | `false` | UC-I policy is not overridden |
| `--debug-icc` | `false` | No ICC diagnostics output |

---

### 2) Policy default (because `--ndk-profile` has a default)

| Option | Default | What it means |
|---|---:|---|
| `--ndk-profile` | `uc-ii` | Default policy profile is **NDK UC-II** |

---

### 3) Derived defaults (depend on `--ndk-profile`)

| Behavior | `mc` | `uc-i` | `uc-ii` |
|---|---|---|---|
| Default `--out-depth` | `b16` | `b8` | `b8` |
| Output ICC allowed by policy | yes | **no** (forced) | yes |
| Default output ICC if `--out-icc` not provided | **preserve input ICC** | *(disabled)* | **sRGB** |
| ICC embedding into output TIFF | yes | no | yes |
| ICC sidecar | only if `--write-icc` is used | only if `--write-icc` is used | only if `--write-icc` is used |

---

## Minimal commands: "what exactly happens"

### A) Absolute minimum invocation

```bash
nclr -i in.tif -o out.tif
```

Because `--ndk-profile` defaults to `uc-ii`, this is effectively:

- **Policy:** `uc-ii`
- **Input profile (colorspace):** embedded ICC if present, else assumed sRGB
- **Output profile (colorspace):** sRGB (default UC-II target if `--out-icc` not set)
- **ICC transform:** enabled (input profile → sRGB) in 16-bit precision
- **Output depth:** `b8` (derived)
- **Tone mapping:** `none` (default)
- **Dithering:** `false` (default)
- **ICC:** embedded into output TIFF (tag 34675)

---

### B) Minimal "UC-I" (books/periodicals, output ICC disabled)

```bash
nclr --ndk-profile uc-i -i in.tif -o out.tif
```

Effective behavior:

- **Output ICC:** none (policy disables output ICC)
- **ICC transform:** not performed by default (because output ICC is disabled)
- **Output depth:** `b8` (derived)
- **Tone mapping:** `none`
- **Dithering:** `false`

If you actually want UC-I to do a controlled transform (e.g. to sRGB) while keeping the UC-I policy intent, override policy:

```bash
nclr --ndk-profile uc-i --force-out-icc -i in.tif -o out.tif
```

---

### C) Minimal "MC" (16-bit output, preserve input ICC by default)

```bash
nclr --ndk-profile mc -i in.tif -o out.tif
```

Effective behavior:

- **Input profile:** embedded ICC if present, else assumed sRGB
- **Output profile:** preserve input ICC by default (unless you set `--out-icc`)
- **ICC:** embedded into output TIFF (tag 34675)
- **Output depth:** `b16`

> Reminder: use MC→MC when you need a *JHOVE-valid* / standards-friendly TIFF rewrite.  
> The rewrite intentionally drops non-core metadata; treat the result as **MC-II**.

# JHOVE TIFF Validation Report (formatted)

## Run information

- **Tool:** Jhove 1.34.0 (report root date: 2025-07-02)
- **Run timestamp:** 2026-01-26T13:29:58+01:00
- **Reporting module:** TIFF-hul 1.9.5 (as reported per file)

## Summary

| File | Status | Size (bytes) | Size (GiB) | WxH | Bits/sample | ICC profile | Profiles |
|---|---|---:|---:|---:|---:|---|---|
| `output-mc.tif` | **Well-Formed and valid** | 1513324962 | 1.409 | 13446×18758 | 16,16,16 | eciRGB v2 |  |
| `output-uc-i.tif` | **Well-Formed and valid** | 756661578 | 0.705 | 13446×18758 | 8,8,8 | (none reported) | Baseline RGB (Class R); DLF Benchmark for Faithful Digital Reproductions of Monographs and Serials: color |
| `output-uc-ii.tif` | **Well-Formed and valid** | 756662178 | 0.705 | 13446×18758 | 8,8,8 | sRGB built-in | Baseline RGB (Class R); DLF Benchmark for Faithful Digital Reproductions of Monographs and Serials: color |
| `source.tif` | **Not well-formed** | 1513553922 | 1.410 |  |  | (none reported) |  |

## output-mc.tif
- **Path:** `C:\temp\jhove\to-validate\output-mc.tif`
- **Status:** **Well-Formed and valid**
- **Format:** TIFF 6.0
- **MIME:** image/tiff
- **Last modified:** 2026-01-26T13:28:57+01:00
- **Size:** 1513324962 bytes (1.409 GiB)
- **MIX (NISO) summary:**
  - Dimensions: **13446×18758**
  - Color space: RGB
  - Compression: Uncompressed
  - Bits per sample: 16, 16, 16
  - ICC profile name: eciRGB v2
  - Sampling: 600×600 in.

## output-uc-i.tif
- **Path:** `C:\temp\jhove\to-validate\output-uc-i.tif`
- **Status:** **Well-Formed and valid**
- **Format:** TIFF 6.0
- **MIME:** image/tiff
- **Last modified:** 2026-01-26T13:29:27+01:00
- **Size:** 756661578 bytes (0.705 GiB)
- **Profiles (TIFF-hul):**
  - Baseline RGB (Class R)
  - DLF Benchmark for Faithful Digital Reproductions of Monographs and Serials: color
- **MIX (NISO) summary:**
  - Dimensions: **13446×18758**
  - Color space: RGB
  - Compression: Uncompressed
  - Bits per sample: 8, 8, 8
  - ICC profile name: *(none reported in MIX)*
  - Sampling: 600×600 in.

## output-uc-ii.tif
- **Path:** `C:\temp\jhove\to-validate\output-uc-ii.tif`
- **Status:** **Well-Formed and valid**
- **Format:** TIFF 6.0
- **MIME:** image/tiff
- **Last modified:** 2026-01-26T13:29:16+01:00
- **Size:** 756662178 bytes (0.705 GiB)
- **Profiles (TIFF-hul):**
  - Baseline RGB (Class R)
  - DLF Benchmark for Faithful Digital Reproductions of Monographs and Serials: color
- **MIX (NISO) summary:**
  - Dimensions: **13446×18758**
  - Color space: RGB
  - Compression: Uncompressed
  - Bits per sample: 8, 8, 8
  - ICC profile name: sRGB built-in
  - Sampling: 600×600 in.

## source.tif
- **Path:** `C:\temp\jhove\to-validate\source.tif`
- **Status:** **Not well-formed**
- **Format:** TIFF
- **MIME:** image/tiff
- **Last modified:** 2026-01-02T08:21:24+01:00
- **Size:** 1513553922 bytes (1.410 GiB)
- **Messages:**
  - **ERROR TIFF-HUL-7:** Type mismatch for tag 41995; expecting 7, saw 2 ([info](https://github.com/openpreserve/jhove/wiki/TIFF-hul-Messages#tiff-hul-7))

## Raw report (XML)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<jhove xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns="http://schema.openpreservation.org/ois/xml/ns/jhove" xsi:schemaLocation="http://schema.openpreservation.org/ois/xml/ns/jhove https://schema.openpreservation.org/ois/xml/xsd/jhove/1.10/jhove.xsd" name="Jhove" release="1.34.0" date="2025-07-02">
 <date>2026-01-26T13:29:58+01:00</date>
 <repInfo uri="C:\temp\jhove\to-validate\output-mc.tif">
  <reportingModule release="1.9.5" date="2024-08-22">TIFF-hul</reportingModule>
  <lastModified>2026-01-26T13:28:57+01:00</lastModified>
  <size>1513324962</size>
  <format>TIFF</format>
  <version>6.0</version>
  <status>Well-Formed and valid</status>
  <sigMatch>
  <module>TIFF-hul</module>
  </sigMatch>
  <mimeType>image/tiff</mimeType>
  <properties>
   <property>
    <name>TIFFMetadata</name>
    <values arity="Array" type="Property">
    <property>
     <name>ByteOrder</name>
     <values arity="Scalar" type="String">
      <value>little-endian</value>
     </values>
    </property>
    <property>
     <name>IFDs</name>
     <values arity="List" type="Property">
     <property>
      <name>Number</name>
      <values arity="Scalar" type="Integer">
       <value>1</value>
      </values>
     </property>
     <property>
      <name>IFD</name>
      <values arity="Array" type="Property">
      <property>
       <name>Offset</name>
       <values arity="Scalar" type="Long">
        <value>1513324788</value>
       </values>
      </property>
      <property>
       <name>Type</name>
       <values arity="Scalar" type="String">
        <value>TIFF</value>
       </values>
      </property>
      <property>
       <name>Entries</name>
       <values arity="List" type="Property">
       <property>
        <name>NisoImageMetadata</name>
        <values arity="Scalar" type="NISOImageMetadata">
         <value>
       <mix:mix xmlns:mix="http://www.loc.gov/mix/v20" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.loc.gov/mix/v20 http://www.loc.gov/standards/mix/mix20/mix20.xsd">
        <mix:BasicDigitalObjectInformation>
         <mix:ObjectIdentifier>
          <mix:objectIdentifierType>JHOVE</mix:objectIdentifierType>
         </mix:ObjectIdentifier>
         <mix:FormatDesignation>
          <mix:formatName>image/tiff</mix:formatName>
         </mix:FormatDesignation>
         <mix:byteOrder>little endian</mix:byteOrder>
         <mix:Compression>
          <mix:compressionScheme>Uncompressed</mix:compressionScheme>
         </mix:Compression>
        </mix:BasicDigitalObjectInformation>
        <mix:BasicImageInformation>
         <mix:BasicImageCharacteristics>
          <mix:imageWidth>13446</mix:imageWidth>
          <mix:imageHeight>18758</mix:imageHeight>
          <mix:PhotometricInterpretation>
           <mix:colorSpace>RGB</mix:colorSpace>
           <mix:ColorProfile>
            <mix:IccProfile>
             <mix:iccProfileName>eciRGB v2</mix:iccProfileName>
            </mix:IccProfile>
           </mix:ColorProfile>
           <mix:ReferenceBlackWhite>
            <mix:Component>
             <mix:componentPhotometricInterpretation>R</mix:componentPhotometricInterpretation>
             <mix:footroom>
              <mix:numerator>0</mix:numerator>
             </mix:footroom>
             <mix:headroom>
              <mix:numerator>65535</mix:numerator>
             </mix:headroom>
             </mix:Component>
            <mix:Component>
             <mix:componentPhotometricInterpretation>G</mix:componentPhotometricInterpretation>
             <mix:footroom>
              <mix:numerator>0</mix:numerator>
             </mix:footroom>
             <mix:headroom>
              <mix:numerator>65535</mix:numerator>
             </mix:headroom>
             </mix:Component>
            <mix:Component>
             <mix:componentPhotometricInterpretation>B</mix:componentPhotometricInterpretation>
             <mix:footroom>
              <mix:numerator>0</mix:numerator>
             </mix:footroom>
             <mix:headroom>
              <mix:numerator>65535</mix:numerator>
             </mix:headroom>
             </mix:Component>
            </mix:ReferenceBlackWhite>
          </mix:PhotometricInterpretation>
         </mix:BasicImageCharacteristics>
        </mix:BasicImageInformation>
        <mix:ImageCaptureMetadata>
         <mix:orientation>normal*</mix:orientation>
        </mix:ImageCaptureMetadata>
        <mix:ImageAssessmentMetadata>
         <mix:SpatialMetrics>
          <mix:samplingFrequencyUnit>in.</mix:samplingFrequencyUnit>
          <mix:xSamplingFrequency>
           <mix:numerator>600</mix:numerator>
          </mix:xSamplingFrequency>
          <mix:ySamplingFrequency>
           <mix:numerator>600</mix:numerator>
          </mix:ySamplingFrequency>
         </mix:SpatialMetrics>
         <mix:ImageColorEncoding>
          <mix:BitsPerSample>
           <mix:bitsPerSampleValue>16</mix:bitsPerSampleValue>
           <mix:bitsPerSampleValue>16</mix:bitsPerSampleValue>
           <mix:bitsPerSampleValue>16</mix:bitsPerSampleValue>
           <mix:bitsPerSampleUnit>integer</mix:bitsPerSampleUnit>
          </mix:BitsPerSample>
          <mix:samplesPerPixel>3</mix:samplesPerPixel>
         </mix:ImageColorEncoding>
        </mix:ImageAssessmentMetadata>
       </mix:mix>
         </value>
        </values>
       </property>
       <property>
        <name>NewSubfileType</name>
        <values arity="Scalar" type="Long">
         <value>0</value>
        </values>
       </property>
       <property>
        <name>SampleFormat</name>
        <values arity="Array" type="Integer">
         <value>1</value>
         <value>1</value>
         <value>1</value>
        </values>
       </property>
       <property>
        <name>MinSampleValue</name>
        <values arity="Array" type="Integer">
         <value>0</value>
         <value>0</value>
         <value>0</value>
        </values>
       </property>
       <property>
        <name>MaxSampleValue</name>
        <values arity="Array" type="Integer">
         <value>65535</value>
         <value>65535</value>
         <value>65535</value>
        </values>
       </property>
       <property>
        <name>TransferRange</name>
        <values arity="Array" type="Integer">
         <value>0</value>
         <value>65535</value>
         <value>0</value>
         <value>65535</value>
         <value>0</value>
         <value>65535</value>
        </values>
       </property>
       <property>
        <name>Threshholding</name>
        <values arity="Scalar" type="Integer">
         <value>1</value>
        </values>
       </property>
       <property>
        <name>StripOffsets</name>
        <values arity="Array" type="Long">
         <value>8</value>
         <value>5163272</value>
         <value>10326536</value>
         <value>15489800</value>
         <value>20653064</value>
         <value>25816328</value>
         <value>30979592</value>
         <value>36142856</value>
         <value>41306120</value>
         <value>46469384</value>
         <value>51632648</value>
         <value>56795912</value>
         <value>61959176</value>
         <value>67122440</value>
         <value>72285704</value>
         <value>77448968</value>
         <value>82612232</value>
         <value>87775496</value>
         <value>92938760</value>
         <value>98102024</value>
         <value>103265288</value>
         <value>108428552</value>
         <value>113591816</value>
         <value>118755080</value>
         <value>123918344</value>
         <value>129081608</value>
         <value>134244872</value>
         <value>139408136</value>
         <value>144571400</value>
         <value>149734664</value>
         <value>154897928</value>
         <value>160061192</value>
         <value>165224456</value>
         <value>170387720</value>
         <value>175550984</value>
         <value>180714248</value>
         <value>185877512</value>
         <value>191040776</value>
         <value>196204040</value>
         <value>201367304</value>
         <value>206530568</value>
         <value>211693832</value>
         <value>216857096</value>
         <value>222020360</value>
         <value>227183624</value>
         <value>232346888</value>
         <value>237510152</value>
         <value>242673416</value>
         <value>247836680</value>
         <value>252999944</value>
         <value>258163208</value>
         <value>263326472</value>
         <value>268489736</value>
         <value>273653000</value>
         <value>278816264</value>
         <value>283979528</value>
         <value>289142792</value>
         <value>294306056</value>
         <value>299469320</value>
         <value>304632584</value>
         <value>309795848</value>
         <value>314959112</value>
         <value>320122376</value>
         <value>325285640</value>
         <value>330448904</value>
         <value>335612168</value>
         <value>340775432</value>
         <value>345938696</value>
         <value>351101960</value>
         <value>356265224</value>
         <value>361428488</value>
         <value>366591752</value>
         <value>371755016</value>
         <value>376918280</value>
         <value>382081544</value>
         <value>387244808</value>
         <value>392408072</value>
         <value>397571336</value>
         <value>402734600</value>
         <value>407897864</value>
         <value>413061128</value>
         <value>418224392</value>
         <value>423387656</value>
         <value>428550920</value>
         <value>433714184</value>
         <value>438877448</value>
         <value>444040712</value>
         <value>449203976</value>
         <value>454367240</value>
         <value>459530504</value>
         <value>464693768</value>
         <value>469857032</value>
         <value>475020296</value>
         <value>480183560</value>
         <value>485346824</value>
         <value>490510088</value>
         <value>495673352</value>
         <value>500836616</value>
         <value>505999880</value>
         <value>511163144</value>
         <value>516326408</value>
         <value>521489672</value>
         <value>526652936</value>
         <value>531816200</value>
         <value>536979464</value>
         <value>542142728</value>
         <value>547305992</value>
         <value>552469256</value>
         <value>557632520</value>
         <value>562795784</value>
         <value>567959048</value>
         <value>573122312</value>
         <value>578285576</value>
         <value>583448840</value>
         <value>588612104</value>
         <value>593775368</value>
         <value>598938632</value>
         <value>604101896</value>
         <value>609265160</value>
         <value>614428424</value>
         <value>619591688</value>
         <value>624754952</value>
         <value>629918216</value>
         <value>635081480</value>
         <value>640244744</value>
         <value>645408008</value>
         <value>650571272</value>
         <value>655734536</value>
         <value>660897800</value>
         <value>666061064</value>
         <value>671224328</value>
         <value>676387592</value>
         <value>681550856</value>
         <value>686714120</value>
         <value>691877384</value>
         <value>697040648</value>
         <value>702203912</value>
         <value>707367176</value>
         <value>712530440</value>
         <value>717693704</value>
         <value>722856968</value>
         <value>728020232</value>
         <value>733183496</value>
         <value>738346760</value>
         <value>743510024</value>
         <value>748673288</value>
         <value>753836552</value>
         <value>758999816</value>
         <value>764163080</value>
         <value>769326344</value>
         <value>774489608</value>
         <value>779652872</value>
         <value>784816136</value>
         <value>789979400</value>
         <value>795142664</value>
         <value>800305928</value>
         <value>805469192</value>
         <value>810632456</value>
         <value>815795720</value>
         <value>820958984</value>
         <value>826122248</value>
         <value>831285512</value>
         <value>836448776</value>
         <value>841612040</value>
         <value>846775304</value>
         <value>851938568</value>
         <value>857101832</value>
         <value>862265096</value>
         <value>867428360</value>
         <value>872591624</value>
         <value>877754888</value>
         <value>882918152</value>
         <value>888081416</value>
         <value>893244680</value>
         <value>898407944</value>
         <value>903571208</value>
         <value>908734472</value>
         <value>913897736</value>
         <value>919061000</value>
         <value>924224264</value>
         <value>929387528</value>
         <value>934550792</value>
         <value>939714056</value>
         <value>944877320</value>
         <value>950040584</value>
         <value>955203848</value>
         <value>960367112</value>
         <value>965530376</value>
         <value>970693640</value>
         <value>975856904</value>
         <value>981020168</value>
         <value>986183432</value>
         <value>991346696</value>
         <value>996509960</value>
         <value>1001673224</value>
         <value>1006836488</value>
         <value>1011999752</value>
         <value>1017163016</value>
         <value>1022326280</value>
         <value>1027489544</value>
         <value>1032652808</value>
         <value>1037816072</value>
         <value>1042979336</value>
         <value>1048142600</value>
         <value>1053305864</value>
         <value>1058469128</value>
         <value>1063632392</value>
         <value>1068795656</value>
         <value>1073958920</value>
         <value>1079122184</value>
         <value>1084285448</value>
         <value>1089448712</value>
         <value>1094611976</value>
         <value>1099775240</value>
         <value>1104938504</value>
         <value>1110101768</value>
         <value>1115265032</value>
         <value>1120428296</value>
         <value>1125591560</value>
         <value>1130754824</value>
         <value>1135918088</value>
         <value>1141081352</value>
         <value>1146244616</value>
         <value>1151407880</value>
         <value>1156571144</value>
         <value>1161734408</value>
         <value>1166897672</value>
         <value>1172060936</value>
         <value>1177224200</value>
         <value>1182387464</value>
         <value>1187550728</value>
         <value>1192713992</value>
         <value>1197877256</value>
         <value>1203040520</value>
         <value>1208203784</value>
         <value>1213367048</value>
         <value>1218530312</value>
         <value>1223693576</value>
         <value>1228856840</value>
         <value>1234020104</value>
         <value>1239183368</value>
         <value>1244346632</value>
         <value>1249509896</value>
         <value>1254673160</value>
         <value>1259836424</value>
         <value>1264999688</value>
         <value>1270162952</value>
         <value>1275326216</value>
         <value>1280489480</value>
         <value>1285652744</value>
         <value>1290816008</value>
         <value>1295979272</value>
         <value>1301142536</value>
         <value>1306305800</value>
         <value>1311469064</value>
         <value>1316632328</value>
         <value>1321795592</value>
         <value>1326958856</value>
         <value>1332122120</value>
         <value>1337285384</value>
         <value>1342448648</value>
         <value>1347611912</value>
         <value>1352775176</value>
         <value>1357938440</value>
         <value>1363101704</value>
         <value>1368264968</value>
         <value>1373428232</value>
         <value>1378591496</value>
         <value>1383754760</value>
         <value>1388918024</value>
         <value>1394081288</value>
         <value>1399244552</value>
         <value>1404407816</value>
         <value>1409571080</value>
         <value>1414734344</value>
         <value>1419897608</value>
         <value>1425060872</value>
         <value>1430224136</value>
         <value>1435387400</value>
         <value>1440550664</value>
         <value>1445713928</value>
         <value>1450877192</value>
         <value>1456040456</value>
         <value>1461203720</value>
         <value>1466366984</value>
         <value>1471530248</value>
         <value>1476693512</value>
         <value>1481856776</value>
         <value>1487020040</value>
         <value>1492183304</value>
         <value>1497346568</value>
         <value>1502509832</value>
         <value>1507673096</value>
         <value>1512836360</value>
        </values>
       </property>
       <property>
        <name>RowsPerStrip</name>
        <values arity="Scalar" type="Long">
         <value>64</value>
        </values>
       </property>
       <property>
        <name>StripByteCounts</name>
        <values arity="Array" type="Long">
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>484056</value>
        </values>
       </property>
       <property>
        <name>PlanarConfiguration</name>
        <values arity="Scalar" type="Integer">
         <value>1</value>
        </values>
       </property>
       <property>
        <name>TIFFITProperties</name>
        <values arity="List" type="Property">
        <property>
         <name>BackgroundColorIndicator</name>
         <values arity="Scalar" type="String">
          <value>background not defined</value>
         </values>
        </property>
        <property>
         <name>ImageColorIndicator</name>
         <values arity="Scalar" type="String">
          <value>image not defined</value>
         </values>
        </property>
        <property>
         <name>TransparencyIndicator</name>
         <values arity="Scalar" type="String">
          <value>no transparency</value>
         </values>
        </property>
        <property>
         <name>PixelIntensityRange</name>
         <values arity="Array" type="Integer">
          <value>0</value>
          <value>65535</value>
         </values>
        </property>
        <property>
         <name>RasterPadding</name>
         <values arity="Scalar" type="String">
          <value>1 byte</value>
         </values>
        </property>
        <property>
         <name>BitsPerRunLength</name>
         <values arity="Scalar" type="Integer">
          <value>8</value>
         </values>
        </property>
        <property>
         <name>BitsPerExtendedRunLength</name>
         <values arity="Scalar" type="Integer">
          <value>16</value>
         </values>
        </property>
        </values>
       </property>
       <property>
        <name>TIFFEPProperties</name>
        <values arity="List" type="Property">
        <property>
         <name>ICCProfile</name>
         <values arity="Scalar" type="Boolean">
          <value>true</value>
         </values>
        </property>
        </values>
       </property>
       </values>
      </property>
      </values>
     </property>
     </values>
    </property>
    </values>
   </property>
  </properties>
 </repInfo>
 <repInfo uri="C:\temp\jhove\to-validate\output-uc-i.tif">
  <reportingModule release="1.9.5" date="2024-08-22">TIFF-hul</reportingModule>
  <lastModified>2026-01-26T13:29:27+01:00</lastModified>
  <size>756661578</size>
  <format>TIFF</format>
  <version>6.0</version>
  <status>Well-Formed and valid</status>
  <sigMatch>
  <module>TIFF-hul</module>
  </sigMatch>
  <mimeType>image/tiff</mimeType>
  <profiles>
   <profile>Baseline RGB (Class R)</profile>
   <profile>DLF Benchmark for Faithful Digital Reproductions of Monographs and Serials: color</profile>
  </profiles>
  <properties>
   <property>
    <name>TIFFMetadata</name>
    <values arity="Array" type="Property">
    <property>
     <name>ByteOrder</name>
     <values arity="Scalar" type="String">
      <value>little-endian</value>
     </values>
    </property>
    <property>
     <name>IFDs</name>
     <values arity="List" type="Property">
     <property>
      <name>Number</name>
      <values arity="Scalar" type="Integer">
       <value>1</value>
      </values>
     </property>
     <property>
      <name>IFD</name>
      <values arity="Array" type="Property">
      <property>
       <name>Offset</name>
       <values arity="Scalar" type="Long">
        <value>756661416</value>
       </values>
      </property>
      <property>
       <name>Type</name>
       <values arity="Scalar" type="String">
        <value>TIFF</value>
       </values>
      </property>
      <property>
       <name>Entries</name>
       <values arity="List" type="Property">
       <property>
        <name>NisoImageMetadata</name>
        <values arity="Scalar" type="NISOImageMetadata">
         <value>
       <mix:mix xmlns:mix="http://www.loc.gov/mix/v20" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.loc.gov/mix/v20 http://www.loc.gov/standards/mix/mix20/mix20.xsd">
        <mix:BasicDigitalObjectInformation>
         <mix:ObjectIdentifier>
          <mix:objectIdentifierType>JHOVE</mix:objectIdentifierType>
         </mix:ObjectIdentifier>
         <mix:FormatDesignation>
          <mix:formatName>image/tiff</mix:formatName>
         </mix:FormatDesignation>
         <mix:byteOrder>little endian</mix:byteOrder>
         <mix:Compression>
          <mix:compressionScheme>Uncompressed</mix:compressionScheme>
         </mix:Compression>
        </mix:BasicDigitalObjectInformation>
        <mix:BasicImageInformation>
         <mix:BasicImageCharacteristics>
          <mix:imageWidth>13446</mix:imageWidth>
          <mix:imageHeight>18758</mix:imageHeight>
          <mix:PhotometricInterpretation>
           <mix:colorSpace>RGB</mix:colorSpace>
           <mix:ReferenceBlackWhite>
            <mix:Component>
             <mix:componentPhotometricInterpretation>R</mix:componentPhotometricInterpretation>
             <mix:footroom>
              <mix:numerator>0</mix:numerator>
             </mix:footroom>
             <mix:headroom>
              <mix:numerator>255</mix:numerator>
             </mix:headroom>
             </mix:Component>
            <mix:Component>
             <mix:componentPhotometricInterpretation>G</mix:componentPhotometricInterpretation>
             <mix:footroom>
              <mix:numerator>0</mix:numerator>
             </mix:footroom>
             <mix:headroom>
              <mix:numerator>255</mix:numerator>
             </mix:headroom>
             </mix:Component>
            <mix:Component>
             <mix:componentPhotometricInterpretation>B</mix:componentPhotometricInterpretation>
             <mix:footroom>
              <mix:numerator>0</mix:numerator>
             </mix:footroom>
             <mix:headroom>
              <mix:numerator>255</mix:numerator>
             </mix:headroom>
             </mix:Component>
            </mix:ReferenceBlackWhite>
          </mix:PhotometricInterpretation>
         </mix:BasicImageCharacteristics>
        </mix:BasicImageInformation>
        <mix:ImageCaptureMetadata>
         <mix:orientation>normal*</mix:orientation>
        </mix:ImageCaptureMetadata>
        <mix:ImageAssessmentMetadata>
         <mix:SpatialMetrics>
          <mix:samplingFrequencyUnit>in.</mix:samplingFrequencyUnit>
          <mix:xSamplingFrequency>
           <mix:numerator>600</mix:numerator>
          </mix:xSamplingFrequency>
          <mix:ySamplingFrequency>
           <mix:numerator>600</mix:numerator>
          </mix:ySamplingFrequency>
         </mix:SpatialMetrics>
         <mix:ImageColorEncoding>
          <mix:BitsPerSample>
           <mix:bitsPerSampleValue>8</mix:bitsPerSampleValue>
           <mix:bitsPerSampleValue>8</mix:bitsPerSampleValue>
           <mix:bitsPerSampleValue>8</mix:bitsPerSampleValue>
           <mix:bitsPerSampleUnit>integer</mix:bitsPerSampleUnit>
          </mix:BitsPerSample>
          <mix:samplesPerPixel>3</mix:samplesPerPixel>
         </mix:ImageColorEncoding>
        </mix:ImageAssessmentMetadata>
       </mix:mix>
         </value>
        </values>
       </property>
       <property>
        <name>NewSubfileType</name>
        <values arity="Scalar" type="Long">
         <value>0</value>
        </values>
       </property>
       <property>
        <name>SampleFormat</name>
        <values arity="Array" type="Integer">
         <value>1</value>
         <value>1</value>
         <value>1</value>
        </values>
       </property>
       <property>
        <name>MinSampleValue</name>
        <values arity="Array" type="Integer">
         <value>0</value>
         <value>0</value>
         <value>0</value>
        </values>
       </property>
       <property>
        <name>MaxSampleValue</name>
        <values arity="Array" type="Integer">
         <value>255</value>
         <value>255</value>
         <value>255</value>
        </values>
       </property>
       <property>
        <name>TransferRange</name>
        <values arity="Array" type="Integer">
         <value>0</value>
         <value>255</value>
         <value>0</value>
         <value>255</value>
         <value>0</value>
         <value>255</value>
        </values>
       </property>
       <property>
        <name>Threshholding</name>
        <values arity="Scalar" type="Integer">
         <value>1</value>
        </values>
       </property>
       <property>
        <name>StripOffsets</name>
        <values arity="Array" type="Long">
         <value>8</value>
         <value>5163272</value>
         <value>10326536</value>
         <value>15489800</value>
         <value>20653064</value>
         <value>25816328</value>
         <value>30979592</value>
         <value>36142856</value>
         <value>41306120</value>
         <value>46469384</value>
         <value>51632648</value>
         <value>56795912</value>
         <value>61959176</value>
         <value>67122440</value>
         <value>72285704</value>
         <value>77448968</value>
         <value>82612232</value>
         <value>87775496</value>
         <value>92938760</value>
         <value>98102024</value>
         <value>103265288</value>
         <value>108428552</value>
         <value>113591816</value>
         <value>118755080</value>
         <value>123918344</value>
         <value>129081608</value>
         <value>134244872</value>
         <value>139408136</value>
         <value>144571400</value>
         <value>149734664</value>
         <value>154897928</value>
         <value>160061192</value>
         <value>165224456</value>
         <value>170387720</value>
         <value>175550984</value>
         <value>180714248</value>
         <value>185877512</value>
         <value>191040776</value>
         <value>196204040</value>
         <value>201367304</value>
         <value>206530568</value>
         <value>211693832</value>
         <value>216857096</value>
         <value>222020360</value>
         <value>227183624</value>
         <value>232346888</value>
         <value>237510152</value>
         <value>242673416</value>
         <value>247836680</value>
         <value>252999944</value>
         <value>258163208</value>
         <value>263326472</value>
         <value>268489736</value>
         <value>273653000</value>
         <value>278816264</value>
         <value>283979528</value>
         <value>289142792</value>
         <value>294306056</value>
         <value>299469320</value>
         <value>304632584</value>
         <value>309795848</value>
         <value>314959112</value>
         <value>320122376</value>
         <value>325285640</value>
         <value>330448904</value>
         <value>335612168</value>
         <value>340775432</value>
         <value>345938696</value>
         <value>351101960</value>
         <value>356265224</value>
         <value>361428488</value>
         <value>366591752</value>
         <value>371755016</value>
         <value>376918280</value>
         <value>382081544</value>
         <value>387244808</value>
         <value>392408072</value>
         <value>397571336</value>
         <value>402734600</value>
         <value>407897864</value>
         <value>413061128</value>
         <value>418224392</value>
         <value>423387656</value>
         <value>428550920</value>
         <value>433714184</value>
         <value>438877448</value>
         <value>444040712</value>
         <value>449203976</value>
         <value>454367240</value>
         <value>459530504</value>
         <value>464693768</value>
         <value>469857032</value>
         <value>475020296</value>
         <value>480183560</value>
         <value>485346824</value>
         <value>490510088</value>
         <value>495673352</value>
         <value>500836616</value>
         <value>505999880</value>
         <value>511163144</value>
         <value>516326408</value>
         <value>521489672</value>
         <value>526652936</value>
         <value>531816200</value>
         <value>536979464</value>
         <value>542142728</value>
         <value>547305992</value>
         <value>552469256</value>
         <value>557632520</value>
         <value>562795784</value>
         <value>567959048</value>
         <value>573122312</value>
         <value>578285576</value>
         <value>583448840</value>
         <value>588612104</value>
         <value>593775368</value>
         <value>598938632</value>
         <value>604101896</value>
         <value>609265160</value>
         <value>614428424</value>
         <value>619591688</value>
         <value>624754952</value>
         <value>629918216</value>
         <value>635081480</value>
         <value>640244744</value>
         <value>645408008</value>
         <value>650571272</value>
         <value>655734536</value>
         <value>660897800</value>
         <value>666061064</value>
         <value>671224328</value>
         <value>676387592</value>
         <value>681550856</value>
         <value>686714120</value>
         <value>691877384</value>
         <value>697040648</value>
         <value>702203912</value>
         <value>707367176</value>
         <value>712530440</value>
         <value>717693704</value>
         <value>722856968</value>
         <value>728020232</value>
         <value>733183496</value>
         <value>738346760</value>
         <value>743510024</value>
         <value>748673288</value>
         <value>753836552</value>
        </values>
       </property>
       <property>
        <name>RowsPerStrip</name>
        <values arity="Scalar" type="Long">
         <value>128</value>
        </values>
       </property>
       <property>
        <name>StripByteCounts</name>
        <values arity="Array" type="Long">
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>2823660</value>
        </values>
       </property>
       <property>
        <name>PlanarConfiguration</name>
        <values arity="Scalar" type="Integer">
         <value>1</value>
        </values>
       </property>
       <property>
        <name>TIFFITProperties</name>
        <values arity="List" type="Property">
        <property>
         <name>BackgroundColorIndicator</name>
         <values arity="Scalar" type="String">
          <value>background not defined</value>
         </values>
        </property>
        <property>
         <name>ImageColorIndicator</name>
         <values arity="Scalar" type="String">
          <value>image not defined</value>
         </values>
        </property>
        <property>
         <name>TransparencyIndicator</name>
         <values arity="Scalar" type="String">
          <value>no transparency</value>
         </values>
        </property>
        <property>
         <name>PixelIntensityRange</name>
         <values arity="Array" type="Integer">
          <value>0</value>
          <value>255</value>
         </values>
        </property>
        <property>
         <name>RasterPadding</name>
         <values arity="Scalar" type="String">
          <value>1 byte</value>
         </values>
        </property>
        <property>
         <name>BitsPerRunLength</name>
         <values arity="Scalar" type="Integer">
          <value>8</value>
         </values>
        </property>
        <property>
         <name>BitsPerExtendedRunLength</name>
         <values arity="Scalar" type="Integer">
          <value>16</value>
         </values>
        </property>
        </values>
       </property>
       </values>
      </property>
      </values>
     </property>
     </values>
    </property>
    </values>
   </property>
  </properties>
 </repInfo>
 <repInfo uri="C:\temp\jhove\to-validate\output-uc-ii.tif">
  <reportingModule release="1.9.5" date="2024-08-22">TIFF-hul</reportingModule>
  <lastModified>2026-01-26T13:29:16+01:00</lastModified>
  <size>756662178</size>
  <format>TIFF</format>
  <version>6.0</version>
  <status>Well-Formed and valid</status>
  <sigMatch>
  <module>TIFF-hul</module>
  </sigMatch>
  <mimeType>image/tiff</mimeType>
  <profiles>
   <profile>Baseline RGB (Class R)</profile>
   <profile>DLF Benchmark for Faithful Digital Reproductions of Monographs and Serials: color</profile>
  </profiles>
  <properties>
   <property>
    <name>TIFFMetadata</name>
    <values arity="Array" type="Property">
    <property>
     <name>ByteOrder</name>
     <values arity="Scalar" type="String">
      <value>little-endian</value>
     </values>
    </property>
    <property>
     <name>IFDs</name>
     <values arity="List" type="Property">
     <property>
      <name>Number</name>
      <values arity="Scalar" type="Integer">
       <value>1</value>
      </values>
     </property>
     <property>
      <name>IFD</name>
      <values arity="Array" type="Property">
      <property>
       <name>Offset</name>
       <values arity="Scalar" type="Long">
        <value>756662004</value>
       </values>
      </property>
      <property>
       <name>Type</name>
       <values arity="Scalar" type="String">
        <value>TIFF</value>
       </values>
      </property>
      <property>
       <name>Entries</name>
       <values arity="List" type="Property">
       <property>
        <name>NisoImageMetadata</name>
        <values arity="Scalar" type="NISOImageMetadata">
         <value>
       <mix:mix xmlns:mix="http://www.loc.gov/mix/v20" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://www.loc.gov/mix/v20 http://www.loc.gov/standards/mix/mix20/mix20.xsd">
        <mix:BasicDigitalObjectInformation>
         <mix:ObjectIdentifier>
          <mix:objectIdentifierType>JHOVE</mix:objectIdentifierType>
         </mix:ObjectIdentifier>
         <mix:FormatDesignation>
          <mix:formatName>image/tiff</mix:formatName>
         </mix:FormatDesignation>
         <mix:byteOrder>little endian</mix:byteOrder>
         <mix:Compression>
          <mix:compressionScheme>Uncompressed</mix:compressionScheme>
         </mix:Compression>
        </mix:BasicDigitalObjectInformation>
        <mix:BasicImageInformation>
         <mix:BasicImageCharacteristics>
          <mix:imageWidth>13446</mix:imageWidth>
          <mix:imageHeight>18758</mix:imageHeight>
          <mix:PhotometricInterpretation>
           <mix:colorSpace>RGB</mix:colorSpace>
           <mix:ColorProfile>
            <mix:IccProfile>
             <mix:iccProfileName>sRGB built-in</mix:iccProfileName>
            </mix:IccProfile>
           </mix:ColorProfile>
           <mix:ReferenceBlackWhite>
            <mix:Component>
             <mix:componentPhotometricInterpretation>R</mix:componentPhotometricInterpretation>
             <mix:footroom>
              <mix:numerator>0</mix:numerator>
             </mix:footroom>
             <mix:headroom>
              <mix:numerator>255</mix:numerator>
             </mix:headroom>
             </mix:Component>
            <mix:Component>
             <mix:componentPhotometricInterpretation>G</mix:componentPhotometricInterpretation>
             <mix:footroom>
              <mix:numerator>0</mix:numerator>
             </mix:footroom>
             <mix:headroom>
              <mix:numerator>255</mix:numerator>
             </mix:headroom>
             </mix:Component>
            <mix:Component>
             <mix:componentPhotometricInterpretation>B</mix:componentPhotometricInterpretation>
             <mix:footroom>
              <mix:numerator>0</mix:numerator>
             </mix:footroom>
             <mix:headroom>
              <mix:numerator>255</mix:numerator>
             </mix:headroom>
             </mix:Component>
            </mix:ReferenceBlackWhite>
          </mix:PhotometricInterpretation>
         </mix:BasicImageCharacteristics>
        </mix:BasicImageInformation>
        <mix:ImageCaptureMetadata>
         <mix:orientation>normal*</mix:orientation>
        </mix:ImageCaptureMetadata>
        <mix:ImageAssessmentMetadata>
         <mix:SpatialMetrics>
          <mix:samplingFrequencyUnit>in.</mix:samplingFrequencyUnit>
          <mix:xSamplingFrequency>
           <mix:numerator>600</mix:numerator>
          </mix:xSamplingFrequency>
          <mix:ySamplingFrequency>
           <mix:numerator>600</mix:numerator>
          </mix:ySamplingFrequency>
         </mix:SpatialMetrics>
         <mix:ImageColorEncoding>
          <mix:BitsPerSample>
           <mix:bitsPerSampleValue>8</mix:bitsPerSampleValue>
           <mix:bitsPerSampleValue>8</mix:bitsPerSampleValue>
           <mix:bitsPerSampleValue>8</mix:bitsPerSampleValue>
           <mix:bitsPerSampleUnit>integer</mix:bitsPerSampleUnit>
          </mix:BitsPerSample>
          <mix:samplesPerPixel>3</mix:samplesPerPixel>
         </mix:ImageColorEncoding>
        </mix:ImageAssessmentMetadata>
       </mix:mix>
         </value>
        </values>
       </property>
       <property>
        <name>NewSubfileType</name>
        <values arity="Scalar" type="Long">
         <value>0</value>
        </values>
       </property>
       <property>
        <name>SampleFormat</name>
        <values arity="Array" type="Integer">
         <value>1</value>
         <value>1</value>
         <value>1</value>
        </values>
       </property>
       <property>
        <name>MinSampleValue</name>
        <values arity="Array" type="Integer">
         <value>0</value>
         <value>0</value>
         <value>0</value>
        </values>
       </property>
       <property>
        <name>MaxSampleValue</name>
        <values arity="Array" type="Integer">
         <value>255</value>
         <value>255</value>
         <value>255</value>
        </values>
       </property>
       <property>
        <name>TransferRange</name>
        <values arity="Array" type="Integer">
         <value>0</value>
         <value>255</value>
         <value>0</value>
         <value>255</value>
         <value>0</value>
         <value>255</value>
        </values>
       </property>
       <property>
        <name>Threshholding</name>
        <values arity="Scalar" type="Integer">
         <value>1</value>
        </values>
       </property>
       <property>
        <name>StripOffsets</name>
        <values arity="Array" type="Long">
         <value>8</value>
         <value>5163272</value>
         <value>10326536</value>
         <value>15489800</value>
         <value>20653064</value>
         <value>25816328</value>
         <value>30979592</value>
         <value>36142856</value>
         <value>41306120</value>
         <value>46469384</value>
         <value>51632648</value>
         <value>56795912</value>
         <value>61959176</value>
         <value>67122440</value>
         <value>72285704</value>
         <value>77448968</value>
         <value>82612232</value>
         <value>87775496</value>
         <value>92938760</value>
         <value>98102024</value>
         <value>103265288</value>
         <value>108428552</value>
         <value>113591816</value>
         <value>118755080</value>
         <value>123918344</value>
         <value>129081608</value>
         <value>134244872</value>
         <value>139408136</value>
         <value>144571400</value>
         <value>149734664</value>
         <value>154897928</value>
         <value>160061192</value>
         <value>165224456</value>
         <value>170387720</value>
         <value>175550984</value>
         <value>180714248</value>
         <value>185877512</value>
         <value>191040776</value>
         <value>196204040</value>
         <value>201367304</value>
         <value>206530568</value>
         <value>211693832</value>
         <value>216857096</value>
         <value>222020360</value>
         <value>227183624</value>
         <value>232346888</value>
         <value>237510152</value>
         <value>242673416</value>
         <value>247836680</value>
         <value>252999944</value>
         <value>258163208</value>
         <value>263326472</value>
         <value>268489736</value>
         <value>273653000</value>
         <value>278816264</value>
         <value>283979528</value>
         <value>289142792</value>
         <value>294306056</value>
         <value>299469320</value>
         <value>304632584</value>
         <value>309795848</value>
         <value>314959112</value>
         <value>320122376</value>
         <value>325285640</value>
         <value>330448904</value>
         <value>335612168</value>
         <value>340775432</value>
         <value>345938696</value>
         <value>351101960</value>
         <value>356265224</value>
         <value>361428488</value>
         <value>366591752</value>
         <value>371755016</value>
         <value>376918280</value>
         <value>382081544</value>
         <value>387244808</value>
         <value>392408072</value>
         <value>397571336</value>
         <value>402734600</value>
         <value>407897864</value>
         <value>413061128</value>
         <value>418224392</value>
         <value>423387656</value>
         <value>428550920</value>
         <value>433714184</value>
         <value>438877448</value>
         <value>444040712</value>
         <value>449203976</value>
         <value>454367240</value>
         <value>459530504</value>
         <value>464693768</value>
         <value>469857032</value>
         <value>475020296</value>
         <value>480183560</value>
         <value>485346824</value>
         <value>490510088</value>
         <value>495673352</value>
         <value>500836616</value>
         <value>505999880</value>
         <value>511163144</value>
         <value>516326408</value>
         <value>521489672</value>
         <value>526652936</value>
         <value>531816200</value>
         <value>536979464</value>
         <value>542142728</value>
         <value>547305992</value>
         <value>552469256</value>
         <value>557632520</value>
         <value>562795784</value>
         <value>567959048</value>
         <value>573122312</value>
         <value>578285576</value>
         <value>583448840</value>
         <value>588612104</value>
         <value>593775368</value>
         <value>598938632</value>
         <value>604101896</value>
         <value>609265160</value>
         <value>614428424</value>
         <value>619591688</value>
         <value>624754952</value>
         <value>629918216</value>
         <value>635081480</value>
         <value>640244744</value>
         <value>645408008</value>
         <value>650571272</value>
         <value>655734536</value>
         <value>660897800</value>
         <value>666061064</value>
         <value>671224328</value>
         <value>676387592</value>
         <value>681550856</value>
         <value>686714120</value>
         <value>691877384</value>
         <value>697040648</value>
         <value>702203912</value>
         <value>707367176</value>
         <value>712530440</value>
         <value>717693704</value>
         <value>722856968</value>
         <value>728020232</value>
         <value>733183496</value>
         <value>738346760</value>
         <value>743510024</value>
         <value>748673288</value>
         <value>753836552</value>
        </values>
       </property>
       <property>
        <name>RowsPerStrip</name>
        <values arity="Scalar" type="Long">
         <value>128</value>
        </values>
       </property>
       <property>
        <name>StripByteCounts</name>
        <values arity="Array" type="Long">
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>5163264</value>
         <value>2823660</value>
        </values>
       </property>
       <property>
        <name>PlanarConfiguration</name>
        <values arity="Scalar" type="Integer">
         <value>1</value>
        </values>
       </property>
       <property>
        <name>TIFFITProperties</name>
        <values arity="List" type="Property">
        <property>
         <name>BackgroundColorIndicator</name>
         <values arity="Scalar" type="String">
          <value>background not defined</value>
         </values>
        </property>
        <property>
         <name>ImageColorIndicator</name>
         <values arity="Scalar" type="String">
          <value>image not defined</value>
         </values>
        </property>
        <property>
         <name>TransparencyIndicator</name>
         <values arity="Scalar" type="String">
          <value>no transparency</value>
         </values>
        </property>
        <property>
         <name>PixelIntensityRange</name>
         <values arity="Array" type="Integer">
          <value>0</value>
          <value>255</value>
         </values>
        </property>
        <property>
         <name>RasterPadding</name>
         <values arity="Scalar" type="String">
          <value>1 byte</value>
         </values>
        </property>
        <property>
         <name>BitsPerRunLength</name>
         <values arity="Scalar" type="Integer">
          <value>8</value>
         </values>
        </property>
        <property>
         <name>BitsPerExtendedRunLength</name>
         <values arity="Scalar" type="Integer">
          <value>16</value>
         </values>
        </property>
        </values>
       </property>
       <property>
        <name>TIFFEPProperties</name>
        <values arity="List" type="Property">
        <property>
         <name>ICCProfile</name>
         <values arity="Scalar" type="Boolean">
          <value>true</value>
         </values>
        </property>
        </values>
       </property>
       </values>
      </property>
      </values>
     </property>
     </values>
    </property>
    </values>
   </property>
  </properties>
 </repInfo>
 <repInfo uri="C:\temp\jhove\to-validate\source.tif">
  <reportingModule release="1.9.5" date="2024-08-22">TIFF-hul</reportingModule>
  <lastModified>2026-01-02T08:21:24+01:00</lastModified>
  <size>1513553922</size>
  <format>TIFF</format>
  <status>Not well-formed</status>
  <sigMatch>
  <module>TIFF-hul</module>
  </sigMatch>
  <messages>
   <message severity="error" id="TIFF-HUL-7" infoLink="https://github.com/openpreserve/jhove/wiki/TIFF-hul-Messages#tiff-hul-7">Type mismatch for tag 41995; expecting 7, saw 2</message>
  </messages>
  <mimeType>image/tiff</mimeType>
 </repInfo>
</jhove>
```

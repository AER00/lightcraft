# Sony embedded distortion corrections

LightCraft reads the signed 16-sample distortion table in the raw image IFD (`0x7037`) of Sony ARWs.
It converts that table into `OpcodeList3` / `WarpRectilinear`, so the existing profile correction control,
CPU/GPU optics path, EXIF orientation handling, and DNG export use the same correction.

This adds **distortion only**. Sony vignetting and lateral chromatic-aberration tables are not decoded.
The tested files are Bayer ARWs from the ILCE-7RM4A with the FE 24–105mm F4 G OSS and
FE 200–600mm F5.6–6.3 G OSS. Linear YCbCr ARWs and non-3:2 camera crops are excluded pending validation. An aspect crop may
retain the full-frame radial normalization; the decoder does not guess that relationship. Older files that carry
only encrypted correction metadata, different table lengths, and rejected tables remain uncorrected.
The camera's distortion Off setting does not erase its table. Newly imported photos use LightCraft's existing
embedded-profile default (enabled); the profile correction control can disable or adjust it.

## Clean-room evidence

The [ExifTool EXIF tag-name documentation](https://exiftool.org/TagNames/EXIF.html) identifies
`0x7037` as 17 signed 16-bit integers. The first value is the count, 16, followed by radial samples.
No external photo decoder implementation or Adobe lens profile was consulted.
Sony Imaging Edge Edit 4.1 was used only as a black-box reference on local scratch copies.

The observed inverse mapping, with radius `r` normalized to the default crop's half diagonal, is:

```
f(r) = 1 + interpolate(table, r) / 16384
source = center + (output - center) * zoom * f(r)
zoom = 1 / max(f(r)) over radii on the rectangular output boundary
```

The border radii range from the shorter half edge / half diagonal to 1. Table knots span 0 through 1.
Linear interpolation approximates the observed map closely; we do not claim to reproduce Sony's
internal interpolator. A fixed-size weighted least-squares fit expresses source radius as
`r * (k0 + k1*r² + k2*r⁴ + k3*r⁶)`. The fit is rejected if radial error exceeds 0.0005 of the
half diagonal (about 0.43 px at a 1440 px 3:2 preview), or the mapping folds. Table values are bounded
and the input must have precisely the supported signed-short layout. Crop offset and normalization
are converted into DNG's active-area coordinate system before creating the opcode. The standard
`DefaultCropOrigin` / `DefaultCropSize` take precedence over Sony crop tags: on the tested A7R IVA
files the former start at raw x=32 and the latter at x=0. The standard origin aligns with Sony
exports; the old precedence shifted content by eight pixels in a 2376-pixel-wide export.

The main raw-IFD table is important: the similarly named MakerNote table has different values and
must not be substituted into this formula. Main raw-IFD and SR2SubIFD values agreed in the test files.

## Validation

At 2376×1584, independent SIFT correspondences between Sony distortion On and Off TIFF exports gave:

| Lens / focal length | Median residual | 95th percentile | Maximum matched radius |
|---|---:|---:|---:|
| FE 200–600 / 200 mm | 0.043 px | 0.191 px | 0.97 half diagonals |
| FE 200–600 / 600 mm | 0.055 px | 0.231 px | 0.95 half diagonals |

These use the fixed 16384 scale and automatic border framing, not fitted per-image scale or zoom.
A median translation below 0.004 px was removed from the residuals. Correspondences do not cover
all pixels; these are geometric feature residuals, not photometric equality or a worst-case guarantee.
Across 20 sampled RAWs (16 at 24–105 mm, four at 200–600 mm), the polynomial's maximum deviation
from the interpolated file table was below 0.12 px at 1440 px. That measures approximation error,
not agreement with an independent renderer. Camera JPEG and Apple Core Image comparisons support
the wide-lens interpretation but are weaker evidence than same-renderer On/Off pairs.

Actual LightCraft exports after crop normalization were also compared directly with Sony's corrected
TIFFs. At 2376 px wide, median / 95th-percentile feature residuals were 0.35 / 1.04 px at 24 mm,
0.15 / 0.40 px at 200 mm, and 0.13 / 0.56 px at 600 mm (median translation below 0.024 px).
The uncorrected medians at 200 and 600 mm were 8.63 and 10.82 px respectively. Different colour,
sharpening, and demosaicing affect feature localization; these are geometry checks, not colour parity.
An isolated desktop run on Apple M1 Metal detected the profile, rendered without GPU fallback or
warnings, and exposed the existing Optics controls. Existing library edits are not reset: once the
source has loaded, enable lens corrections if they were previously disabled.

Tests cover signedness/length rejection, zero and malformed tables, independent 600 mm reference
geometry, barrel framing, offset crop normalization, both TIFF byte orders, header/full consistency,
and preservation through DNG export. No private photographs or reference TIFFs are committed.

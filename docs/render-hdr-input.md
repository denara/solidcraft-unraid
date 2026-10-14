# Bounded Radiance input

`solidcraft_render::lighting` provides a caller-owned cooperative RGBE decoder.
This is input preparation only: it enables no environment import command, document
format, PBR material, lighting capability, GPU upload or image-render backend.
The existing canvas decoder explicitly accepts PNG/JPEG only, so Cargo feature
unification cannot send HDR through its synchronous whole-image loading path.
It is the independent input foundation for #813 / #620, coordinated with the
maintainer's renderer work described in #792.

`HdrSource::new(Vec<u8>)` checks a bounded header using the existing `image`
metadata codec. Success does **not** mean the pixel stream is valid. Use
`validate(cancelled)` for a synchronous full-stream check, or construct
`HdrDecodeTask` and repeatedly call `step(units, cancelled)` for cooperative
preparation. Only `Complete(DecodedHdr)` contains publishable pixels. Cancellation
and errors release partial buffers; subsequent calls return `Consumed`, including
after successful completion. The cancellation callback must itself be bounded.
The consumer owns scheduling and must not perform synchronous `validate` on a
latency-sensitive UI thread.

Input limits are 16 MiB of encoded Vec capacity, a 64 KiB header, 2048x1024 and
2,097,152 pixels. Only 2:1, square-pixel, standard `-Y +X` RGBE is accepted.
EXR, XYZE and other orientations are unsupported. A call accepts 1–16,384 work
units; each unit decodes one packet (at most 128 component writes) or converts one
pixel. An overall work cap bounds malformed old-style runs. Corrected channels
must be finite, nonnegative and at most 100,000,000. Limits are intentional preview
policy, not a statement about all valid Radiance files.

The float pixel allocation is at most 24 MiB, with one RGBE scanline at most 8 KiB
and a shared encoded source at most 16 MiB. Construction reserves float storage
fallibly; initialization happens incrementally. This is a per-task estimate,
not a process memory quota. The consumer must admit concurrent jobs, source
ownership, decoded copies, colour conversion, GPU textures and IBL maps within
its own total budget before starting.

RGBE conversion follows `image` 0.25.10's mantissa/exponent convention, with no
tone mapping or clipping to [0,1]. Stored values are divided by header EXPOSURE
and COLORCORR. Physical units are not guaranteed by arbitrary input files.
`HdrInfo::primaries` preserves explicit PRIMARIES chromaticities; absence is
unspecified. Output is linear RGB in the source colour space, **not implicitly
linear sRGB**. Consumers must choose and validate a working-space transform;
basic finite/range validation of chromaticities here does not prove an invertible
or physically meaningful gamut. No gamut conversion is performed by this utility.

The scanline adapter is derived from the permissively licensed `image` codec;
see ATTRIBUTION.md, NOTICE and THIRD-PARTY-LICENSES.txt. Test images are original
inline byte fixtures. No commercial-product images or materials are included.

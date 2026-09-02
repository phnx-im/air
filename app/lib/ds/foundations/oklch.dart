// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:math' as math;
import 'dart:ui';

/// A color in the OKLab space: perceptual lightness, and the a/b axes.
class Oklab {
  const Oklab(this.l, this.a, this.b);

  final double l;
  final double a;
  final double b;
}

/// A color in the OKLCH space: lightness, chroma, and hue in degrees
/// (`[0, 360)`).
class Oklch {
  const Oklch(this.l, this.c, this.h);

  final double l;
  final double c;
  final double h;
}

/// Linearizes one sRGB channel (`[0, 1]`).
double _srgbToLinear(double c) {
  return c <= 0.04045
      ? c / 12.92
      : math.pow((c + 0.055) / 1.055, 2.4).toDouble();
}

/// Gamma-encodes one linear sRGB channel (`[0, 1]`).
double _linearToSrgb(double c) {
  return c <= 0.0031308
      ? c * 12.92
      : 1.055 * math.pow(c, 1 / 2.4).toDouble() - 0.055;
}

double _cbrt(double x) =>
    x < 0 ? -math.pow(-x, 1 / 3).toDouble() : math.pow(x, 1 / 3).toDouble();

double _clamp01(double x) => x < 0 ? 0 : (x > 1 ? 1 : x);

/// Linear (not gamma-encoded, not gamut-clamped) sRGB components.
class _LinearRgb {
  const _LinearRgb(this.r, this.g, this.b);

  final double r;
  final double g;
  final double b;
}

Oklab _linearRgbToOklab(_LinearRgb rgb) {
  final l = 0.4122214708 * rgb.r + 0.5363325363 * rgb.g + 0.0514459929 * rgb.b;
  final m = 0.2119034982 * rgb.r + 0.6806995451 * rgb.g + 0.1073969566 * rgb.b;
  final s = 0.0883024619 * rgb.r + 0.2817188376 * rgb.g + 0.6299787005 * rgb.b;

  final l_ = _cbrt(l);
  final m_ = _cbrt(m);
  final s_ = _cbrt(s);

  return Oklab(
    0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
    1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
    0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
  );
}

_LinearRgb _oklabToLinearRgb(Oklab lab) {
  final l_ = lab.l + 0.3963377774 * lab.a + 0.2158037573 * lab.b;
  final m_ = lab.l - 0.1055613458 * lab.a - 0.0638541728 * lab.b;
  final s_ = lab.l - 0.0894841775 * lab.a - 1.2914855480 * lab.b;

  final l = l_ * l_ * l_;
  final m = m_ * m_ * m_;
  final s = s_ * s_ * s_;

  return _LinearRgb(
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
  );
}

bool _inGamut(_LinearRgb rgb) {
  const eps = 1e-4;
  return rgb.r >= -eps &&
      rgb.r <= 1 + eps &&
      rgb.g >= -eps &&
      rgb.g <= 1 + eps &&
      rgb.b >= -eps &&
      rgb.b <= 1 + eps;
}

Color _colorFromLinearRgb(_LinearRgb rgb) {
  final r = (_linearToSrgb(_clamp01(rgb.r)) * 255).round();
  final g = (_linearToSrgb(_clamp01(rgb.g)) * 255).round();
  final b = (_linearToSrgb(_clamp01(rgb.b)) * 255).round();
  return Color.fromARGB(255, r, g, b);
}

/// Reduces chroma at fixed lightness and hue until the color is in the sRGB
/// gamut, then converts it. Keeps [Oklch.toColor] and [Oklab.toColor] gamut
/// safe for arbitrary inputs.
Color _gamutSafeToColor(Oklch oklch) {
  var rgb = _oklabToLinearRgb(oklch.toOklab());
  if (_inGamut(rgb)) return _colorFromLinearRgb(rgb);

  var lo = 0.0;
  var hi = oklch.c;
  for (var i = 0; i < 12; i++) {
    final mid = (lo + hi) / 2;
    final candidate = _oklabToLinearRgb(Oklch(oklch.l, mid, oklch.h).toOklab());
    if (_inGamut(candidate)) {
      lo = mid;
    } else {
      hi = mid;
    }
  }
  rgb = _oklabToLinearRgb(Oklch(oklch.l, lo, oklch.h).toOklab());
  return _colorFromLinearRgb(rgb);
}

/// Reads a [Color] as OKLab / OKLCH.
extension ColorOklab on Color {
  Oklab toOklab() {
    final rgb = _LinearRgb(
      _srgbToLinear(r),
      _srgbToLinear(g),
      _srgbToLinear(b),
    );
    return _linearRgbToOklab(rgb);
  }

  Oklch toOklch() => toOklab().toOklch();
}

extension OklabToColor on Oklab {
  Color toColor() => toOklch().toColor();

  Oklch toOklch() {
    final chroma = math.sqrt(a * a + b * b);
    var hue = math.atan2(b, a) * 180 / math.pi;
    if (hue < 0) hue += 360;
    return Oklch(l, chroma, hue);
  }
}

extension OklchToColor on Oklch {
  /// Gamut safe: out-of-range colors are pulled back in by reducing chroma
  /// at fixed lightness and hue.
  Color toColor() => _gamutSafeToColor(this);

  Oklab toOklab() {
    final hRad = h * math.pi / 180;
    return Oklab(l, c * math.cos(hRad), c * math.sin(hRad));
  }
}

/// Linearly interpolates between two OKLab colors.
Oklab lerpOklab(Oklab a, Oklab b, double t) {
  return Oklab(
    a.l + (b.l - a.l) * t,
    a.a + (b.a - a.a) * t,
    a.b + (b.b - a.b) * t,
  );
}

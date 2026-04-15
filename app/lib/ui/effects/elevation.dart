// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

/// Elevation/Small from the design system. Invisible layers (≤1% opacity)
/// are omitted for performance.
const List<BoxShadow> smallElevationBoxShadows = [
  BoxShadow(color: Color(0x0A000000), offset: Offset(0, 19), blurRadius: 12),
  BoxShadow(color: Color(0x12000000), offset: Offset(0, 9), blurRadius: 9),
  BoxShadow(color: Color(0x14000000), offset: Offset(0, 2), blurRadius: 5),
];

/// Elevation/Medium from the design system. Invisible layers (≤1% opacity)
/// are omitted for performance.
const List<BoxShadow> mediumElevationBoxShadows = [
  BoxShadow(color: Color(0x0D000000), offset: Offset(0, 48), blurRadius: 29),
  BoxShadow(color: Color(0x17000000), offset: Offset(0, 21), blurRadius: 21),
  BoxShadow(color: Color(0x1A000000), offset: Offset(0, 5), blurRadius: 12),
];

/// Elevation/Large from the design system. Invisible layers (≤1% opacity)
/// are omitted for performance.
const List<BoxShadow> largeElevationBoxShadows = [
  BoxShadow(color: Color(0x0D000000), offset: Offset(0, 64), blurRadius: 38),
  BoxShadow(color: Color(0x17000000), offset: Offset(0, 28), blurRadius: 28),
  BoxShadow(color: Color(0x1A000000), offset: Offset(0, 7), blurRadius: 16),
];

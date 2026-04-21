// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/animation.dart';

/// Shared easing for all motion tokens.
const Curve motionEasing = Cubic(0.25, 1.0, 0.5, 1.0);

const Duration motionInstant = Duration(milliseconds: 80);
const Duration motionShort = Duration(milliseconds: 160);
const Duration motionRegular = Duration(milliseconds: 240);
const Duration motionMedium = Duration(milliseconds: 400);
const Duration motionLong = Duration(milliseconds: 600);

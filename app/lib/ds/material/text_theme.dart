// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/ds/foundations/foundations.dart';

/// Material's text slots mapped onto the design-system typescale.
TextTheme get customTextScheme => TextTheme(
  displayLarge: typeScale.header.xl.style(),
  displayMedium: typeScale.header.l.style(),
  displaySmall: typeScale.header.m.style(),
  headlineLarge: typeScale.header.regular.style(),
  headlineMedium: typeScale.header.s.style(),
  headlineSmall: typeScale.header.xs.style(),
  titleLarge: typeScale.header.regular.style(),
  titleMedium: typeScale.body.s.style(),
  titleSmall: typeScale.body.xs.style(),
  bodyLarge: typeScale.body.regular.style(),
  bodyMedium: typeScale.body.s.style(),
  bodySmall: typeScale.body.xs.style(),
  labelLarge: typeScale.body.s.style(),
  labelMedium: typeScale.body.s.style(),
  labelSmall: typeScale.body.xs.style(),
);

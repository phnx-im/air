// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:air/ui/typography/font_size.dart';

bool get isCupertino => Platform.isIOS || Platform.isMacOS;

final customTextScheme = TextTheme(
  displayLarge: TextStyle(
    fontSize: HeaderFontSize.h1.size,
    letterSpacing: isCupertino ? HeaderCupertinoTracking.h1.spacing : null,
  ),
  displayMedium: TextStyle(
    fontSize: HeaderFontSize.h2.size,
    letterSpacing: isCupertino ? HeaderCupertinoTracking.h2.spacing : null,
  ),
  displaySmall: TextStyle(
    fontSize: HeaderFontSize.h3.size,
    letterSpacing: isCupertino ? HeaderCupertinoTracking.h3.spacing : null,
  ),
  headlineLarge: TextStyle(
    fontSize: HeaderFontSize.h4.size,
    letterSpacing: isCupertino ? HeaderCupertinoTracking.h4.spacing : null,
  ),
  headlineMedium: TextStyle(
    fontSize: HeaderFontSize.h5.size,
    letterSpacing: isCupertino ? HeaderCupertinoTracking.h5.spacing : null,
  ),
  headlineSmall: TextStyle(
    fontSize: HeaderFontSize.h6.size,
    letterSpacing: isCupertino ? HeaderCupertinoTracking.h6.spacing : null,
  ),
  titleLarge: TextStyle(
    fontSize: HeaderFontSize.h4.size,
    letterSpacing: isCupertino ? HeaderCupertinoTracking.h4.spacing : null,
  ),
  titleMedium: TextStyle(
    fontSize: LabelFontSize.small1.size,
    letterSpacing: isCupertino ? LabelCupertinoTracking.small1.spacing : null,
  ),
  titleSmall: TextStyle(
    fontSize: LabelFontSize.small2.size,
    letterSpacing: isCupertino ? LabelCupertinoTracking.small2.spacing : null,
  ),
  bodyLarge: TextStyle(
    fontSize: BodyFontSize.base.size,
    letterSpacing: isCupertino ? BodyCupertinoTracking.base.spacing : null,
  ),
  bodyMedium: TextStyle(
    fontSize: BodyFontSize.small1.size,
    letterSpacing: isCupertino ? BodyCupertinoTracking.small1.spacing : null,
  ),
  bodySmall: TextStyle(
    fontSize: BodyFontSize.small2.size,
    letterSpacing: isCupertino ? BodyCupertinoTracking.small2.spacing : null,
  ),
  labelLarge: TextStyle(
    fontSize: LabelFontSize.small1.size,
    letterSpacing: isCupertino ? LabelCupertinoTracking.small1.spacing : null,
  ),
  labelMedium: TextStyle(
    fontSize: LabelFontSize.small1.size,
    letterSpacing: isCupertino ? LabelCupertinoTracking.small2.spacing : null,
  ),
  labelSmall: TextStyle(
    fontSize: LabelFontSize.small2.size,
    letterSpacing: isCupertino ? LabelCupertinoTracking.small2.spacing : null,
  ),
);

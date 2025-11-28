// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:flutter/widgets.dart';

/// Different screen types
enum ResponsiveScreenType {
  /// Small screen
  mobile,

  /// Large screen with touch device
  tablet,

  /// Large screen with pointer device
  desktop,
}

ResponsiveScreenType _screenType(double width) {
  if (width < 600) {
    return ResponsiveScreenType.mobile;
  } else if (ResponsiveScreen.isTouch) {
    return ResponsiveScreenType.tablet;
  } else {
    return ResponsiveScreenType.desktop;
  }
}

extension BuildContextScreenTypeExtension on BuildContext {
  ResponsiveScreenType get responsiveScreenType =>
      _screenType(MediaQuery.of(this).size.width);
}

extension BoxConstraintsScreenTypeExtension on BoxConstraints {
  ResponsiveScreenType get screenType => _screenType(maxWidth);
}

class ResponsiveScreen extends StatefulWidget {
  const ResponsiveScreen({
    super.key,
    required this.mobile,
    required this.tablet,
    required this.desktop,
  });

  /// Mobile layout: less than 800px
  final Widget mobile;

  /// Tablet layout: greates than 800px and is touch device (iOS or Android)
  final Widget tablet;

  /// Desktop layout: greater than 800px and is not touch device (macOS, Windows, Linux)
  final Widget desktop;

  static bool isMobile(BuildContext context) =>
      context.responsiveScreenType == ResponsiveScreenType.mobile;
  static bool isTablet(BuildContext context) =>
      context.responsiveScreenType == ResponsiveScreenType.tablet;
  static bool isDesktop(BuildContext context) =>
      context.responsiveScreenType == ResponsiveScreenType.desktop;

  static bool isTouch = Platform.isIOS || Platform.isAndroid;

  @override
  State<ResponsiveScreen> createState() => _ResponsiveScreenState();
}

class _ResponsiveScreenState extends State<ResponsiveScreen> {
  String previousLayout = "";

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, BoxConstraints constraints) =>
          switch (constraints.screenType) {
            ResponsiveScreenType.mobile => widget.mobile,
            ResponsiveScreenType.tablet => widget.tablet,
            ResponsiveScreenType.desktop => widget.desktop,
          },
    );
  }
}

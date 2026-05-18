// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';
import 'dart:math' show min;

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

/// Breakpoint between the mobile (tab bar) and non-mobile (sidebar) layouts.
///
/// Applied to the *shortest* side of the window so that orientation changes
/// don't flip the layout (which means tearing down the declarative page stack
/// in [AppRouterDelegate], which in turn means discarding any pageless routes
/// pushed on top like the attachment upload preview!).
const double kMobileBreakpoint = 576;

ResponsiveScreenType _screenType(double shortestSide) {
  if (shortestSide < kMobileBreakpoint) {
    return ResponsiveScreenType.mobile;
  } else if (ResponsiveScreen.isTouch) {
    return ResponsiveScreenType.tablet;
  } else {
    return ResponsiveScreenType.desktop;
  }
}

extension BuildContextScreenTypeExtension on BuildContext {
  ResponsiveScreenType get responsiveScreenType =>
      _screenType(MediaQuery.of(this).size.shortestSide);
}

extension BoxConstraintsScreenTypeExtension on BoxConstraints {
  ResponsiveScreenType get screenType => _screenType(min(maxWidth, maxHeight));
}

class ResponsiveScreen extends StatefulWidget {
  const ResponsiveScreen({
    super.key,
    required this.mobile,
    required this.tablet,
    required this.desktop,
  });

  /// Mobile layout: width below [kMobileBreakpoint].
  final Widget mobile;

  /// Tablet layout: width at or above [kMobileBreakpoint] on a touch device (iOS, Android).
  final Widget tablet;

  /// Desktop layout: width at or above [kMobileBreakpoint] on a pointer device (macOS, Windows, Linux).
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

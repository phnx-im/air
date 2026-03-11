// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

enum BaseFontPlatform {
  ios(17.0),
  android(16.0),
  macos(13.0),
  windows(15.0),
  linux(15.0);

  final double base;
  const BaseFontPlatform(this.base);
}

// Font sizes for iOS and Android (SF Pro tracking in the comments)
enum MobileFontSizes {
  large5(30.63),
  large4(27.23),
  large3(24.21),
  large2(21.52),
  large1(19.13),
  base(17.00),
  small1(15.11),
  small2(13.43),
  small3(11.93);

  final double size;
  const MobileFontSizes(this.size);
}

// Font sizes for widescreen (macOS, Linux and Windows)
enum WidescreenFontSizes {
  large5(27.03),
  large4(24.03),
  large3(21.36),
  large2(19),
  large1(16.875),
  base(15.00),
  small1(13.335),
  small2(11.85),
  small3(10.53);

  final double size;
  const WidescreenFontSizes(this.size);
}

enum LabelFontSize {
  large2,
  large1,
  base,
  small1,
  small2,
  small3;

  double get size {
    final isDesktop =
        Platform.isMacOS || Platform.isLinux || Platform.isWindows;
    return switch (this) {
      LabelFontSize.large2 =>
        isDesktop
            ? WidescreenFontSizes.large2.size
            : MobileFontSizes.large2.size,
      LabelFontSize.large1 =>
        isDesktop
            ? WidescreenFontSizes.large1.size
            : MobileFontSizes.large1.size,
      LabelFontSize.base =>
        isDesktop ? WidescreenFontSizes.base.size : MobileFontSizes.base.size,
      LabelFontSize.small1 =>
        isDesktop
            ? WidescreenFontSizes.small1.size
            : MobileFontSizes.small1.size,
      LabelFontSize.small2 =>
        isDesktop
            ? WidescreenFontSizes.small2.size
            : MobileFontSizes.small2.size,
      LabelFontSize.small3 =>
        isDesktop
            ? WidescreenFontSizes.small3.size
            : MobileFontSizes.small3.size,
    };
  }
}

enum BodyFontSize {
  large2,
  large1,
  base,
  small1,
  small2,
  small3;

  double get size {
    final isDesktop =
        Platform.isMacOS || Platform.isLinux || Platform.isWindows;
    return switch (this) {
      BodyFontSize.large2 =>
        isDesktop
            ? WidescreenFontSizes.large2.size
            : MobileFontSizes.large2.size,
      BodyFontSize.large1 =>
        isDesktop
            ? WidescreenFontSizes.large1.size
            : MobileFontSizes.large1.size,
      BodyFontSize.base =>
        isDesktop ? WidescreenFontSizes.base.size : MobileFontSizes.base.size,
      BodyFontSize.small1 =>
        isDesktop
            ? WidescreenFontSizes.small1.size
            : MobileFontSizes.small1.size,
      BodyFontSize.small2 =>
        isDesktop
            ? WidescreenFontSizes.small2.size
            : MobileFontSizes.small2.size,
      BodyFontSize.small3 =>
        isDesktop
            ? WidescreenFontSizes.small3.size
            : MobileFontSizes.small3.size,
    };
  }
}

enum HeaderFontSize {
  h1,
  h2,
  h3,
  h4,
  h5,
  h6;

  double get size {
    final isDesktop =
        Platform.isMacOS || Platform.isLinux || Platform.isWindows;
    return switch (this) {
      HeaderFontSize.h1 =>
        isDesktop
            ? WidescreenFontSizes.large5.size
            : MobileFontSizes.large5.size,
      HeaderFontSize.h2 =>
        isDesktop
            ? WidescreenFontSizes.large4.size
            : MobileFontSizes.large4.size,
      HeaderFontSize.h3 =>
        isDesktop
            ? WidescreenFontSizes.large3.size
            : MobileFontSizes.large3.size,
      HeaderFontSize.h4 =>
        isDesktop
            ? WidescreenFontSizes.large2.size
            : MobileFontSizes.large2.size,
      HeaderFontSize.h5 =>
        isDesktop
            ? WidescreenFontSizes.large1.size
            : MobileFontSizes.large1.size,
      HeaderFontSize.h6 =>
        isDesktop ? WidescreenFontSizes.base.size : MobileFontSizes.base.size,
    };
  }
}

enum LabelCupertinoTracking {
  large2(-0.36),
  large1(-0.45),
  base(-0.43),
  small1(-0.23),
  small2(-0.08),
  small3(0.0);

  final double spacing;
  const LabelCupertinoTracking(this.spacing);
}

enum BodyCupertinoTracking {
  large2(-0.36),
  large1(-0.45),
  base(-0.43),
  small1(-0.23),
  small2(-0.08),
  small3(0.0);

  final double spacing;
  const BodyCupertinoTracking(this.spacing);
}

enum HeaderCupertinoTracking {
  h1(-0.40),
  h2(-0.29),
  h3(-0.07),
  h4(-0.36),
  h5(-0.45),
  h6(-0.43);

  final double spacing;
  const HeaderCupertinoTracking(this.spacing);
}

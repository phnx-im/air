// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

enum BaseFontPlatform {
  ios(17.0),
  android(16.0),
  macos(13.0),
  windows(15.0),
  linux(15.0);

  final double base;
  const BaseFontPlatform(this.base);
}

// Font sizes for iOS (SF Pro tracking in the comments)
enum FontSizes {
  large5(30.63), // 0.40
  large4(27.23), // 0.29
  large3(24.21), // 0.07
  large2(21.52), // -0.36
  large1(19.13), // -0.45
  base(17.00), // -0.43
  small1(15.11), // -0.23
  small2(13.43), // -0.08
  small3(11.93); // 0.00

  final double size;
  const FontSizes(this.size);
}

enum LabelFontSize {
  large2(FontSizes.large2),
  large1(FontSizes.large1),
  base(FontSizes.base),
  small1(FontSizes.small1),
  small2(FontSizes.small2),
  small3(FontSizes.small3);

  final FontSizes ref;
  const LabelFontSize(this.ref);
  double get size => ref.size;
}

enum BodyFontSize {
  large2(FontSizes.large2),
  large1(FontSizes.large1),
  base(FontSizes.base),
  small1(FontSizes.small1),
  small2(FontSizes.small2),
  small3(FontSizes.small3);

  final FontSizes ref;
  const BodyFontSize(this.ref);
  double get size => ref.size;
}

enum HeaderFontSize {
  h1(FontSizes.large5),
  h2(FontSizes.large4),
  h3(FontSizes.large3),
  h4(FontSizes.large2),
  h5(FontSizes.large1),
  h6(FontSizes.base);

  final FontSizes ref;
  const HeaderFontSize(this.ref);
  double get size => ref.size;
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

// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/util/cached_memory_image.dart';

class UserAvatar extends StatelessWidget {
  const UserAvatar({
    super.key,
    required this.displayName,
    this.size = 24.0,
    this.image,
    this.onPressed,
  });

  final String displayName;
  final double size;
  final ImageData? image;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onPressed,
      child: MouseRegion(
        cursor: onPressed != null
            ? SystemMouseCursors.click
            : SystemMouseCursors.basic,
        child: SizedBox(
          width: size,
          height: size,
          child: CircleAvatar(
            radius: size / 2,
            backgroundColor: CustomColorScheme.of(context).text.quaternary,
            foregroundImage: image != null
                ? CachedMemoryImage.fromImageData(image!)
                : null,
            child: Text(
              displayName.characters.firstOrNull?.toUpperCase() ?? "",
              style: TextStyle(
                color: CustomColorScheme.of(context).function.white,
                fontSize: LabelFontSize.small2.size * size / 24,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),
        ),
      ),
    );
  }
}

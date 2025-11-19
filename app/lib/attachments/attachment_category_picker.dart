// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:flutter/material.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;

typedef AttachmentCategoryCallback = void Function(AttachmentCategory category);

enum AttachmentCategory { gallery, camera, file }

class AttachmentCategoryPicker extends StatelessWidget {
  const AttachmentCategoryPicker({super.key, this.onCategorySelected});

  final AttachmentCategoryCallback? onCategorySelected;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);
    final iconColor = colors.text.primary;
    final isMobile = Platform.isAndroid || Platform.isIOS;
    final loc = AppLocalizations.of(context);
    return Row(
      mainAxisAlignment: .spaceEvenly,
      children: [
        _AttachmentCategoryButton(
          icon: iconoir.MediaImage(color: iconColor),
          label: loc.attachment_gallery,
          onPressed: () {
            onCategorySelected?.call(AttachmentCategory.gallery);
          },
        ),
        if (isMobile)
          _AttachmentCategoryButton(
            icon: iconoir.Camera(color: iconColor),
            label: loc.attachment_camera,
            onPressed: () {
              onCategorySelected?.call(AttachmentCategory.camera);
            },
          ),
        _AttachmentCategoryButton(
          icon: iconoir.Attachment(color: iconColor),
          label: loc.attachment_file,
          onPressed: () {
            onCategorySelected?.call(AttachmentCategory.file);
          },
        ),
      ],
    );
  }
}

class _AttachmentCategoryButton extends StatelessWidget {
  const _AttachmentCategoryButton({
    required this.icon,
    required this.label,
    this.onPressed,
  });

  final Widget icon;
  final String label;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        SizedBox(
          width: 64,
          height: 64,
          child: IconButton(
            icon: icon,
            style: IconButton.styleFrom(
              backgroundColor: CustomColorScheme.of(
                context,
              ).backgroundElevated.secondary,
            ),
            onPressed: onPressed,
          ),
        ),
        const SizedBox(height: Spacings.xxs),
        Text(label),
        const SizedBox(height: Spacings.xxs),
      ],
    );
  }
}

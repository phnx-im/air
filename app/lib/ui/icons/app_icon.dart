// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

/// Centralized icon catalog for the app.
///
/// Icons are treated as squares, so a single `size` controls both width
/// and height.
enum AppIconType {
  plus,
  close,
  arrowLeft,
  send,
  warningCircle,
  upload,
  mediaImage,
  camera,
  attachment,
  prohibition,
  mediaImagePlus,
  checkSquare,
  square,
  search,
  copy,
  editPencil,
  download,
  shareIos,
  refreshDouble,
  trash,
  arrowRight,
  chatBubbleEmpty,
  check,
  brokenImage,
  personOutline,
  settingsOutline,
  refresh,
  changeCircle,
  logout,
  textSnippet,
  fileDownload,
  fileUpload,
  delete;

  /// Lookup by the enum's `name` (e.g., "plus").
  static AppIconType? fromName(String name) {
    try {
      return AppIconType.values.byName(name);
    } catch (_) {
      return null;
    }
  }
}

class AppIcon extends StatelessWidget {
  const AppIcon({required this.type, super.key, this.color, this.size});

  final AppIconType type;
  final Color? color;
  final double? size;

  @override
  Widget build(BuildContext context) {
    final iconSize = size;

    return switch (type) {
      AppIconType.plus => Icon(
        LucideIcons.plus300,
        size: iconSize,
        color: color,
      ),
      AppIconType.close => Icon(LucideIcons.x, size: iconSize, color: color),
      AppIconType.arrowLeft => Icon(
        LucideIcons.arrowLeft,
        size: iconSize,
        color: color,
      ),
      AppIconType.send => Icon(
        LucideIcons.arrowUp300,
        size: iconSize,
        color: color,
      ),
      AppIconType.warningCircle => Icon(
        LucideIcons.circleAlert,
        size: iconSize,
        color: color,
      ),
      AppIconType.upload => Icon(
        LucideIcons.upload,
        size: iconSize,
        color: color,
      ),
      AppIconType.mediaImage => Icon(
        LucideIcons.image,
        size: iconSize,
        color: color,
      ),
      AppIconType.camera => Icon(
        LucideIcons.camera,
        size: iconSize,
        color: color,
      ),
      AppIconType.attachment => Icon(
        LucideIcons.paperclip,
        size: iconSize,
        color: color,
      ),
      AppIconType.prohibition => Icon(
        LucideIcons.ban,
        size: iconSize,
        color: color,
      ),
      AppIconType.mediaImagePlus => Icon(
        LucideIcons.imagePlus,
        size: iconSize,
        color: color,
      ),
      AppIconType.checkSquare => Icon(
        LucideIcons.squareCheck,
        size: iconSize,
        color: color,
      ),
      AppIconType.square => Icon(
        LucideIcons.square,
        size: iconSize,
        color: color,
      ),
      AppIconType.search => Icon(
        LucideIcons.search,
        size: iconSize,
        color: color,
      ),
      AppIconType.copy => Icon(LucideIcons.copy, size: iconSize, color: color),
      AppIconType.editPencil => Icon(
        LucideIcons.pencil,
        size: iconSize,
        color: color,
      ),
      AppIconType.download => Icon(
        LucideIcons.download,
        size: iconSize,
        color: color,
      ),
      AppIconType.shareIos => Icon(
        LucideIcons.share,
        size: iconSize,
        color: color,
      ),
      AppIconType.refreshDouble => Icon(
        LucideIcons.refreshCw,
        size: iconSize,
        color: color,
      ),
      AppIconType.trash => Icon(
        LucideIcons.trash,
        size: iconSize,
        color: color,
      ),
      AppIconType.arrowRight => Icon(
        LucideIcons.arrowRight,
        size: iconSize,
        color: color,
      ),
      AppIconType.chatBubbleEmpty => Icon(
        LucideIcons.messageCircle,
        size: iconSize,
        color: color,
      ),
      AppIconType.check => Icon(
        LucideIcons.check,
        size: iconSize,
        color: color,
      ),
      AppIconType.brokenImage => Icon(
        Icons.broken_image_outlined,
        color: color,
        size: iconSize,
      ),
      AppIconType.personOutline => Icon(
        LucideIcons.user,
        size: iconSize,
        color: color,
      ),
      AppIconType.settingsOutline => Icon(
        LucideIcons.settings,
        size: iconSize,
        color: color,
      ),
      AppIconType.refresh => Icon(
        LucideIcons.refreshCcw,
        size: iconSize,
        color: color,
      ),
      AppIconType.changeCircle => Icon(
        LucideIcons.repeat,
        size: iconSize,
        color: color,
      ),
      AppIconType.logout => Icon(
        LucideIcons.logOut,
        size: iconSize,
        color: color,
      ),
      AppIconType.textSnippet => Icon(
        LucideIcons.fileText,
        size: iconSize,
        color: color,
      ),
      AppIconType.fileDownload => Icon(
        LucideIcons.fileDown,
        size: iconSize,
        color: color,
      ),
      AppIconType.fileUpload => Icon(
        LucideIcons.fileUp,
        size: iconSize,
        color: color,
      ),
      AppIconType.delete => Icon(
        LucideIcons.trash2,
        size: iconSize,
        color: color,
      ),
    };
  }
}

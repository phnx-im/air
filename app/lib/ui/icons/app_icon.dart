// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';

import 'generated_svg_icons.dart';

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
      AppIconType.plus => compiledSvgWidget(
        CompiledSvgIcon.plus,
        size: iconSize,
        color: color,
      ),
      AppIconType.close => compiledSvgWidget(
        CompiledSvgIcon.x,
        size: iconSize,
        color: color,
      ),
      AppIconType.arrowLeft => compiledSvgWidget(
        CompiledSvgIcon.arrowLeft,
        size: iconSize,
        color: color,
      ),
      AppIconType.send => compiledSvgWidget(
        CompiledSvgIcon.send,
        size: iconSize,
        color: color,
      ),
      AppIconType.warningCircle => compiledSvgWidget(
        CompiledSvgIcon.circleAlert,
        size: iconSize,
        color: color,
      ),
      AppIconType.upload => compiledSvgWidget(
        CompiledSvgIcon.upload,
        size: iconSize,
        color: color,
      ),
      AppIconType.mediaImage => compiledSvgWidget(
        CompiledSvgIcon.image,
        size: iconSize,
        color: color,
      ),
      AppIconType.camera => compiledSvgWidget(
        CompiledSvgIcon.camera,
        size: iconSize,
        color: color,
      ),
      AppIconType.attachment => compiledSvgWidget(
        CompiledSvgIcon.paperclip,
        size: iconSize,
        color: color,
      ),
      AppIconType.prohibition => compiledSvgWidget(
        CompiledSvgIcon.ban,
        size: iconSize,
        color: color,
      ),
      AppIconType.mediaImagePlus => compiledSvgWidget(
        CompiledSvgIcon.imagePlus,
        size: iconSize,
        color: color,
      ),
      AppIconType.checkSquare => compiledSvgWidget(
        CompiledSvgIcon.squareCheck,
        size: iconSize,
        color: color,
      ),
      AppIconType.square => compiledSvgWidget(
        CompiledSvgIcon.square,
        size: iconSize,
        color: color,
      ),
      AppIconType.search => compiledSvgWidget(
        CompiledSvgIcon.search,
        size: iconSize,
        color: color,
      ),
      AppIconType.copy => compiledSvgWidget(
        CompiledSvgIcon.copy,
        size: iconSize,
        color: color,
      ),
      AppIconType.editPencil => compiledSvgWidget(
        CompiledSvgIcon.pencil,
        size: iconSize,
        color: color,
      ),
      AppIconType.download => compiledSvgWidget(
        CompiledSvgIcon.download,
        size: iconSize,
        color: color,
      ),
      AppIconType.shareIos => compiledSvgWidget(
        CompiledSvgIcon.share,
        size: iconSize,
        color: color,
      ),
      AppIconType.refreshDouble => compiledSvgWidget(
        CompiledSvgIcon.refreshCw,
        size: iconSize,
        color: color,
      ),
      AppIconType.trash => compiledSvgWidget(
        CompiledSvgIcon.trash,
        size: iconSize,
        color: color,
      ),
      AppIconType.arrowRight => compiledSvgWidget(
        CompiledSvgIcon.arrowRight,
        size: iconSize,
        color: color,
      ),
      AppIconType.chatBubbleEmpty => compiledSvgWidget(
        CompiledSvgIcon.messageCircle,
        size: iconSize,
        color: color,
      ),
      AppIconType.check => compiledSvgWidget(
        CompiledSvgIcon.check,
        size: iconSize,
        color: color,
      ),
      AppIconType.brokenImage => compiledSvgWidget(
        CompiledSvgIcon.imageOff,
        size: iconSize,
        color: color,
      ),
      AppIconType.personOutline => compiledSvgWidget(
        CompiledSvgIcon.user,
        size: iconSize,
        color: color,
      ),
      AppIconType.settingsOutline => compiledSvgWidget(
        CompiledSvgIcon.settings,
        size: iconSize,
        color: color,
      ),
      AppIconType.refresh => compiledSvgWidget(
        CompiledSvgIcon.refreshCcw,
        size: iconSize,
        color: color,
      ),
      AppIconType.changeCircle => compiledSvgWidget(
        CompiledSvgIcon.repeat,
        size: iconSize,
        color: color,
      ),
      AppIconType.logout => compiledSvgWidget(
        CompiledSvgIcon.logOut,
        size: iconSize,
        color: color,
      ),
      AppIconType.textSnippet => compiledSvgWidget(
        CompiledSvgIcon.fileText,
        size: iconSize,
        color: color,
      ),
      AppIconType.fileDownload => compiledSvgWidget(
        CompiledSvgIcon.fileDown,
        size: iconSize,
        color: color,
      ),
      AppIconType.fileUpload => compiledSvgWidget(
        CompiledSvgIcon.fileUp,
        size: iconSize,
        color: color,
      ),
      AppIconType.delete => compiledSvgWidget(
        CompiledSvgIcon.trash2,
        size: iconSize,
        color: color,
      ),
    };
  }
}

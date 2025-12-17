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
      AppIconType.plus => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.plus, size: iconSize, color: color),
      AppIconType.close => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.x, size: iconSize, color: color),
      AppIconType.arrowLeft => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.arrowLeft, size: iconSize, color: color),
      AppIconType.send => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.send, size: iconSize, color: color),
      AppIconType.warningCircle => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.circleAlert, size: iconSize, color: color),
      AppIconType.upload => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.upload, size: iconSize, color: color),
      AppIconType.mediaImage => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.image, size: iconSize, color: color),
      AppIconType.camera => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.camera, size: iconSize, color: color),
      AppIconType.attachment => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.paperclip, size: iconSize, color: color),
      AppIconType.prohibition => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.ban, size: iconSize, color: color),
      AppIconType.mediaImagePlus => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.imagePlus, size: iconSize, color: color),
      AppIconType.checkSquare => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.squareCheck, size: iconSize, color: color),
      AppIconType.square => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.square, size: iconSize, color: color),
      AppIconType.search => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.search, size: iconSize, color: color),
      AppIconType.copy => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.copy, size: iconSize, color: color),
      AppIconType.editPencil => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.pencil, size: iconSize, color: color),
      AppIconType.download => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.download, size: iconSize, color: color),
      AppIconType.shareIos => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.share, size: iconSize, color: color),
      AppIconType.refreshDouble => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.refreshCw, size: iconSize, color: color),
      AppIconType.trash => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.trash, size: iconSize, color: color),
      AppIconType.arrowRight => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.arrowRight, size: iconSize, color: color),
      AppIconType.chatBubbleEmpty => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.messageCircle, size: iconSize, color: color),
      AppIconType.check => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.check, size: iconSize, color: color),
      AppIconType.brokenImage => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.imageOff, size: iconSize, color: color),
      AppIconType.personOutline => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.user, size: iconSize, color: color),
      AppIconType.settingsOutline => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.settings, size: iconSize, color: color),
      AppIconType.refresh => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.refreshCcw, size: iconSize, color: color),
      AppIconType.changeCircle => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.repeat, size: iconSize, color: color),
      AppIconType.logout => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.logOut, size: iconSize, color: color),
      AppIconType.textSnippet => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.fileText, size: iconSize, color: color),
      AppIconType.fileDownload => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.fileDown, size: iconSize, color: color),
      AppIconType.fileUpload => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.fileUp, size: iconSize, color: color),
      AppIconType.delete => CompiledSvgIconWidget(
          icon: CompiledSvgIcon.trash2, size: iconSize, color: color),
    };
  }
}

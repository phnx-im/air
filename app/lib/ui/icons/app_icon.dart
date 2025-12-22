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
  arrowUp,
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
  shield,
  copy,
  editPencil,
  download,
  shareIos,
  refreshDouble,
  trash,
  arrowRight,
  chatBubbleEmpty,
  check,
  checkCheck,
  checkCheckFill,
  brokenImage,
  user,
  users,
  settingsOutline,
  refresh,
  changeCircle,
  logout,
  textSnippet,
  fileDownload,
  fileUpload,
  delete,
  circleDashed,
}

class AppIcon extends StatelessWidget {
  const AppIcon({
    super.key,
    required this.type,
    required this.color,
    this.size,
  });

  final AppIconType type;
  final Color color;
  final double? size;

  @override
  Widget build(BuildContext context) {
    final iconSize = size;

    return switch (type) {
      AppIconType.plus => CompiledSvgIcon(
        icon: CompiledSvgAsset.plus,
        size: iconSize,
        color: color,
      ),
      AppIconType.close => CompiledSvgIcon(
        icon: CompiledSvgAsset.x,
        size: iconSize,
        color: color,
      ),
      AppIconType.arrowLeft => CompiledSvgIcon(
        icon: CompiledSvgAsset.arrowLeft,
        size: iconSize,
        color: color,
      ),
      AppIconType.arrowUp => CompiledSvgIcon(
        icon: CompiledSvgAsset.arrowUp,
        size: iconSize,
        color: color,
      ),
      AppIconType.send => CompiledSvgIcon(
        icon: CompiledSvgAsset.send,
        size: iconSize,
        color: color,
      ),
      AppIconType.warningCircle => CompiledSvgIcon(
        icon: CompiledSvgAsset.circleAlert,
        size: iconSize,
        color: color,
      ),
      AppIconType.upload => CompiledSvgIcon(
        icon: CompiledSvgAsset.upload,
        size: iconSize,
        color: color,
      ),
      AppIconType.mediaImage => CompiledSvgIcon(
        icon: CompiledSvgAsset.image,
        size: iconSize,
        color: color,
      ),
      AppIconType.camera => CompiledSvgIcon(
        icon: CompiledSvgAsset.camera,
        size: iconSize,
        color: color,
      ),
      AppIconType.attachment => CompiledSvgIcon(
        icon: CompiledSvgAsset.paperclip,
        size: iconSize,
        color: color,
      ),
      AppIconType.prohibition => CompiledSvgIcon(
        icon: CompiledSvgAsset.ban,
        size: iconSize,
        color: color,
      ),
      AppIconType.mediaImagePlus => CompiledSvgIcon(
        icon: CompiledSvgAsset.imagePlus,
        size: iconSize,
        color: color,
      ),
      AppIconType.checkSquare => CompiledSvgIcon(
        icon: CompiledSvgAsset.squareCheck,
        size: iconSize,
        color: color,
      ),
      AppIconType.square => CompiledSvgIcon(
        icon: CompiledSvgAsset.square,
        size: iconSize,
        color: color,
      ),
      AppIconType.search => CompiledSvgIcon(
        icon: CompiledSvgAsset.search,
        size: iconSize,
        color: color,
      ),
      AppIconType.shield => CompiledSvgIcon(
        icon: CompiledSvgAsset.shield,
        size: iconSize,
        color: color,
      ),
      AppIconType.copy => CompiledSvgIcon(
        icon: CompiledSvgAsset.copy,
        size: iconSize,
        color: color,
      ),
      AppIconType.editPencil => CompiledSvgIcon(
        icon: CompiledSvgAsset.pencil,
        size: iconSize,
        color: color,
      ),
      AppIconType.download => CompiledSvgIcon(
        icon: CompiledSvgAsset.download,
        size: iconSize,
        color: color,
      ),
      AppIconType.shareIos => CompiledSvgIcon(
        icon: CompiledSvgAsset.share,
        size: iconSize,
        color: color,
      ),
      AppIconType.refreshDouble => CompiledSvgIcon(
        icon: CompiledSvgAsset.refreshCw,
        size: iconSize,
        color: color,
      ),
      AppIconType.trash => CompiledSvgIcon(
        icon: CompiledSvgAsset.trash,
        size: iconSize,
        color: color,
      ),
      AppIconType.arrowRight => CompiledSvgIcon(
        icon: CompiledSvgAsset.arrowRight,
        size: iconSize,
        color: color,
      ),
      AppIconType.chatBubbleEmpty => CompiledSvgIcon(
        icon: CompiledSvgAsset.messageCircle,
        size: iconSize,
        color: color,
      ),
      AppIconType.check => CompiledSvgIcon(
        icon: CompiledSvgAsset.check,
        size: iconSize,
        color: color,
      ),
      AppIconType.checkCheck => CompiledSvgIcon(
        icon: CompiledSvgAsset.checkCheck,
        size: iconSize,
        color: color,
      ),
      AppIconType.checkCheckFill => CompiledSvgIcon(
        icon: CompiledSvgAsset.checkCheckFill,
        size: iconSize,
        color: color,
      ),
      AppIconType.brokenImage => CompiledSvgIcon(
        icon: CompiledSvgAsset.imageOff,
        size: iconSize,
        color: color,
      ),
      AppIconType.user => CompiledSvgIcon(
        icon: CompiledSvgAsset.user,
        size: iconSize,
        color: color,
      ),
      AppIconType.users => CompiledSvgIcon(
        icon: CompiledSvgAsset.users,
        size: iconSize,
        color: color,
      ),
      AppIconType.settingsOutline => CompiledSvgIcon(
        icon: CompiledSvgAsset.settings,
        size: iconSize,
        color: color,
      ),
      AppIconType.refresh => CompiledSvgIcon(
        icon: CompiledSvgAsset.refreshCcw,
        size: iconSize,
        color: color,
      ),
      AppIconType.changeCircle => CompiledSvgIcon(
        icon: CompiledSvgAsset.repeat,
        size: iconSize,
        color: color,
      ),
      AppIconType.logout => CompiledSvgIcon(
        icon: CompiledSvgAsset.logOut,
        size: iconSize,
        color: color,
      ),
      AppIconType.textSnippet => CompiledSvgIcon(
        icon: CompiledSvgAsset.fileText,
        size: iconSize,
        color: color,
      ),
      AppIconType.fileDownload => CompiledSvgIcon(
        icon: CompiledSvgAsset.fileDown,
        size: iconSize,
        color: color,
      ),
      AppIconType.fileUpload => CompiledSvgIcon(
        icon: CompiledSvgAsset.fileUp,
        size: iconSize,
        color: color,
      ),
      AppIconType.delete => CompiledSvgIcon(
        icon: CompiledSvgAsset.trash2,
        size: iconSize,
        color: color,
      ),
      AppIconType.circleDashed => CompiledSvgIcon(
        icon: CompiledSvgAsset.circleDashed,
        size: iconSize,
        color: color,
      ),
    };
  }
}

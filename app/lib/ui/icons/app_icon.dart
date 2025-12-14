// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;

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
      AppIconType.plus => iconoir.Plus(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.close => iconoir.Xmark(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.arrowLeft => iconoir.ArrowLeft(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.send => iconoir.Send(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.warningCircle => iconoir.WarningCircle(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.upload => iconoir.Upload(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.mediaImage => iconoir.MediaImage(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.camera => iconoir.Camera(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.attachment => iconoir.Attachment(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.prohibition => iconoir.Prohibition(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.mediaImagePlus => iconoir.MediaImagePlus(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.checkSquare => iconoir.CheckSquare(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.square => iconoir.Square(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.search => iconoir.Search(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.copy => iconoir.Copy(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.editPencil => iconoir.EditPencil(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.download => iconoir.Download(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.shareIos => iconoir.ShareIos(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.refreshDouble => iconoir.RefreshDouble(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.trash => iconoir.Trash(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.arrowRight => iconoir.ArrowRight(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.chatBubbleEmpty => iconoir.ChatBubbleEmpty(
        width: iconSize,
        height: iconSize,
        color: color,
      ),
      AppIconType.check => Icon(Icons.check, color: color, size: iconSize),
      AppIconType.brokenImage => Icon(
        Icons.broken_image_outlined,
        color: color,
        size: iconSize,
      ),
      AppIconType.personOutline => Icon(
        Icons.person_outline_rounded,
        color: color,
        size: iconSize,
      ),
      AppIconType.settingsOutline => Icon(
        Icons.settings_outlined,
        color: color,
        size: iconSize,
      ),
      AppIconType.refresh => Icon(Icons.refresh, color: color, size: iconSize),
      AppIconType.changeCircle => Icon(
        Icons.change_circle,
        color: color,
        size: iconSize,
      ),
      AppIconType.logout => Icon(Icons.logout, color: color, size: iconSize),
      AppIconType.textSnippet => Icon(
        Icons.text_snippet,
        color: color,
        size: iconSize,
      ),
      AppIconType.fileDownload => Icon(
        Icons.file_download,
        color: color,
        size: iconSize,
      ),
      AppIconType.fileUpload => Icon(
        Icons.file_upload,
        color: color,
        size: iconSize,
      ),
      AppIconType.delete => Icon(Icons.delete, color: color, size: iconSize),
    };
  }
}

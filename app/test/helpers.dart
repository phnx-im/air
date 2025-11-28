// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/core/core.dart';
import 'package:uuid/uuid.dart';

extension IntTestExtension on int {
  ChatId chatId() => ChatId(uuid: _intToUuidValue(this));

  MessageId messageId() => MessageId(uuid: _intToUuidValue(this));

  /// Convert an int to a [ClientId].
  UiUserId userId({String domain = "localhost"}) =>
      UiUserId(uuid: _intToUuidValue(this), domain: domain);

  AttachmentId attachmentId() => AttachmentId(uuid: _intToUuidValue(this));
}

UuidValue _intToUuidValue(int value) {
  // Convert int to 16-byte array
  final bytes = Uint8List(16)
    ..buffer.asByteData().setInt64(0, value, Endian.little);
  return UuidValue.fromByteList(bytes);
}

class LocalFileComparatorWithThreshold extends LocalFileComparator {
  LocalFileComparatorWithThreshold(super.testFile, this.threshold);

  final double threshold;

  String _platformSuffix() {
    if (Platform.isMacOS) return '.macos';
    if (Platform.isWindows) return '.windows';
    if (Platform.isLinux) return '.linux';
    if (Platform.isAndroid) return '.android';
    if (Platform.isIOS) return '.ios';
    return '';
  }

  @override
  Uri getTestUri(Uri key, int? version) {
    final path = key.toFilePath();
    final newPath = path.replaceFirst(
      RegExp(r'\.png$'),
      '${_platformSuffix()}.png',
    );
    return Uri.file(newPath);
  }

  @override
  Future<bool> compare(Uint8List imageBytes, Uri golden) async {
    final result = await GoldenFileComparator.compareLists(
      imageBytes,
      await getGoldenBytes(golden),
    );
    if (!result.passed && result.diffPercent < threshold) {
      if ((result.diffPercent - threshold).abs() > 0.01) {
        final diff = (result.diffPercent * 10000.0).round() / 100.0;
        // ignore: avoid_print
        print(
          "Golden file comparison passed with $diff% difference, "
          "which is more than 1%pt under the configured threshold of ${threshold * 100}%. "
          "Consider making the threshold tighter.",
        );
      }
      return true;
    } else if (!result.passed) {
      final error = await generateFailureOutput(result, golden, basedir);
      throw FlutterError(error);
    } else {
      return result.passed;
    }
  }
}

// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/widgets/app_bar_x_button.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;
import 'package:photo_view/photo_view.dart';

class AttachmentUploadView extends HookWidget {
  const AttachmentUploadView({
    super.key,
    required this.title,
    required this.file,
    required this.onUpload,
  });

  final String title;
  final XFile file;
  final VoidCallback onUpload;

  @override
  Widget build(BuildContext context) {
    final loadedFile = useMemoized(() => File(file.path), [file]);

    final colors = darkCustomColorScheme;

    return Scaffold(
      backgroundColor: colors.function.black,
      body: Focus(
        autofocus: true,
        onKeyEvent: (node, event) {
          if (event.logicalKey == LogicalKeyboardKey.escape &&
              event is KeyDownEvent) {
            Navigator.pop(context);
            return KeyEventResult.handled;
          }
          return KeyEventResult.ignored;
        },
        child: GestureDetector(
          behavior: HitTestBehavior.translucent,
          child: Stack(
            fit: StackFit.expand,
            children: [
              PhotoView(imageProvider: FileImage(loadedFile)),

              Positioned(
                bottom: Spacings.s,
                right: Spacings.s,
                child: SafeArea(
                  child: IconButton(
                    style: ButtonStyle(
                      backgroundColor: WidgetStatePropertyAll(
                        colors.backgroundBase.secondary,
                      ),
                    ),
                    icon: iconoir.Send(
                      width: 32,
                      height: 32,
                      color: colors.text.primary,
                    ),
                    onPressed: () {
                      onUpload();
                      Navigator.of(context).pop();
                    },
                  ),
                ),
              ),

              Positioned(
                top: 0,
                left: 0,
                right: 0,
                child: Container(
                  color: darkCustomColorScheme.backgroundElevated.primary
                      .withValues(alpha: 0.7),
                  child: AppBar(
                    automaticallyImplyLeading: false,
                    title: Text(
                      title,
                      style: TextStyle(color: colors.text.primary),
                    ),
                    actions: [
                      AppBarXButton(
                        onPressed: () => Navigator.of(context).maybePop(),
                        foregroundColor: colors.text.primary,
                        backgroundColor:
                            darkCustomColorScheme.backgroundBase.secondary,
                      ),
                    ],
                    backgroundColor: Colors.transparent,
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

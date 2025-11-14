// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/attachments/attachments.dart';
import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/widgets/app_bar_back_button.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:iconoir_flutter/iconoir_flutter.dart' as iconoir;
import 'package:photo_view/photo_view.dart';

class ImageViewer extends HookWidget {
  const ImageViewer({
    required this.attachment,
    required this.imageMetadata,
    required this.isSender,
    super.key,
  });

  final UiAttachment attachment;
  final UiImageMetadata imageMetadata;
  final bool isSender;

  @override
  Widget build(BuildContext context) {
    final appBarIsHidden = useState(false);

    return Scaffold(
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
          onTap: () => appBarIsHidden.value = !appBarIsHidden.value,
          child: Stack(
            children: [
              _ZoomablePhotoView(attachment: attachment),

              // AppBar is placed on top of the fullscreen image
              Positioned(
                top: 0,
                left: 0,
                right: 0,
                child: IgnorePointer(
                  ignoring: appBarIsHidden.value,
                  child: AnimatedOpacity(
                    duration: const Duration(milliseconds: 300),
                    opacity: appBarIsHidden.value ? 0 : 1,
                    child: AppBar(
                      leading: const AppBarBackButton(),
                      backgroundColor: CustomColorScheme.of(
                        context,
                      ).backgroundBase.primary.withValues(alpha: 0.7),
                      actions: [
                        IconButton(
                          icon: iconoir.MoreHoriz(
                            width: 32,
                            color: CustomColorScheme.of(context).text.primary,
                          ),
                          color: CustomColorScheme.of(context).text.primary,
                          onPressed: () {
                            // currently nothing to show: later show a context menu
                          },
                        ),
                        const SizedBox(width: Spacings.s),
                      ],
                      title: Text(attachment.filename),
                      centerTitle: true,
                    ),
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

/// PhotoView that can be zoomed in and out with a scroll gesture.
///
/// This is not supported by the widget yet. See
/// <https://github.com/bluefireteam/photo_view/issues/481>
class _ZoomablePhotoView extends HookWidget {
  const _ZoomablePhotoView({required this.attachment});

  final UiAttachment attachment;

  @override
  Widget build(BuildContext context) {
    final photoViewController = useMemoized(() => PhotoViewController());

    // Initial scale set once when photoViewController has a scale value != null
    final initialScale = useState<double?>(null);
    useEffect(() {
      StreamSubscription? subscription;
      subscription = photoViewController.outputStateStream.listen((value) {
        if (value.scale != null && initialScale.value == null) {
          initialScale.value = value.scale;
          subscription?.cancel();
        }
      });
      return () => subscription?.cancel();
    }, [photoViewController]);

    return Listener(
      onPointerSignal: (event) {
        if (event is PointerScrollEvent) {
          final delta = event.scrollDelta.dy;
          final controller = photoViewController;
          final scale = controller.scale;
          if (scale == null || initialScale.value == null) {
            return;
          }
          final newScale = scale - delta / 1000;
          controller.scale = newScale.clamp(initialScale.value! / 2, 10.0);
        }
      },
      child: PhotoView(
        controller: photoViewController,
        filterQuality: FilterQuality.medium,
        imageProvider: AttachmentImageProvider(
          attachment: attachment,
          attachmentsRepository: RepositoryProvider.of(context),
        ),
      ),
    );
  }
}

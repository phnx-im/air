// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/primitives.dart';
import 'package:flutter/material.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/foundations/semantic_colors.dart';
import 'package:air/ds/foundations/type_scale.dart';
import 'package:air/util/cached_memory_image.dart';
import 'package:provider/provider.dart';
import 'package:uuid/uuid.dart';

class UserAvatar extends StatelessWidget {
  const UserAvatar({
    super.key,
    required this.profile,
    this.size = 24.0,
    this.onPressed,
    this.showInitials = true,
    this.showImage = true,
  });

  final UiUserProfile profile;
  final double size;
  final VoidCallback? onPressed;
  final bool showInitials;
  final bool showImage;

  @override
  Widget build(BuildContext context) {
    return _Avatar(
      displayName: showInitials ? profile.displayName : "",
      image: showImage ? profile.profilePicture : null,
      size: size,
      onPressed: onPressed,
      gradientKey: profile.userId.uuid,
    );
  }
}

class ChatAvatar extends StatelessWidget {
  const ChatAvatar({super.key, this.chatId, this.size = 24.0, this.onPressed});

  final ChatId? chatId;
  final double size;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final chat = context.select((ChatDetailsCubit cubit) {
      final details = cubit.state.chat;
      if (chatId != null && details?.id != chatId) {
        return null;
      }
      return details;
    });

    final showImage = switch (chat?.chatType) {
      UiChatType_Connection() || UiChatType_Group() => true,
      _ => false,
    };

    final displayName = chat?.title ?? chat?.displayName ?? "";
    final image = chat?.picture;
    final gradientKey = chat?.userId?.uuid ?? chat?.id.uuid;

    return _Avatar(
      displayName: displayName,
      image: showImage ? image : null,
      size: size,
      onPressed: onPressed,
      gradientKey: gradientKey,
    );
  }
}

class _Avatar extends StatelessWidget {
  const _Avatar({
    required this.displayName,
    required this.image,
    required this.size,
    required this.onPressed,
    required this.gradientKey,
  });

  final String displayName;
  final ImageData? image;
  final double size;
  final VoidCallback? onPressed;
  final UuidValue? gradientKey;

  @override
  Widget build(BuildContext context) {
    final targetSize = (size * MediaQuery.devicePixelRatioOf(context)).round();
    final foregroundImage = image != null
        ? CachedMemoryImage.fromImageData(
            image!,
            targetWidth: targetSize,
            targetHeight: targetSize,
          )
        : null;
    final colors = SemanticColors.of(context);
    final gradient = _AvatarGradient.fromUuid(gradientKey);

    return GestureDetector(
      onTap: onPressed,
      child: MouseRegion(
        cursor: onPressed != null
            ? SystemMouseCursors.click
            : MouseCursor.defer,
        child: SizedBox(
          width: size,
          height: size,
          child: DecoratedBox(
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              gradient: foregroundImage == null
                  ? LinearGradient(
                      colors: gradient.colors,
                      begin: Alignment.topLeft,
                      end: Alignment.bottomRight,
                    )
                  : null,
              color: foregroundImage != null ? colors.text.quaternary : null,
            ),
            child: CircleAvatar(
              radius: size / 2,
              backgroundColor: Colors.transparent,
              foregroundImage: foregroundImage,
              child: Text(
                displayName.characters.firstOrNull?.toUpperCase() ?? "",
                style: TextTheme.of(context).labelMedium!.copyWith(
                  color: colors.function.neutral.white,
                  fontSize: typeScale.body.xs.fontSize * size / 28,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _AvatarGradient {
  const _AvatarGradient({required this.start, required this.end});

  final Color start;
  final Color end;

  List<Color> get colors => [start, end];

  factory _AvatarGradient.fromUuid(UuidValue? uuid) {
    final index = _gradientIndexForUuid(uuid);
    final (start, end) = _gradients[index];
    return _AvatarGradient(start: start, end: end);
  }

  static const _start = Shade.s300;
  static const _end = Shade.s700;

  /// One gradient per chromatic hue, in palette order. That order is
  /// load-bearing: [_gradientIndexForUuid] indexes into this list, so adding a
  /// hue to [Hue] re-colors existing avatars.
  static final _gradients = [
    for (final hue in Hue.values)
      (Primitives.chromatic(hue, _start), Primitives.chromatic(hue, _end)),
  ];

  static int _gradientIndexForUuid(UuidValue? uuid) {
    if (uuid == null) {
      return 0;
    }
    // Cheap uniformity inspired by Java's String.hashCode()
    var hash = 0;
    for (final codeUnit in uuid.uuid.codeUnits) {
      hash = ((hash << 5) + hash) + codeUnit;
      hash &= 0xFFFFFFFF;
    }
    return hash % _gradients.length;
  }
}

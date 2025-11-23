// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ui/colors/palette.dart';
import 'package:flutter/material.dart';
import 'package:air/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/user/user.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/util/cached_memory_image.dart';
import 'package:provider/provider.dart';
import 'package:uuid/uuid.dart';

class UserAvatar extends StatelessWidget {
  const UserAvatar({super.key, this.userId, this.size = 24.0, this.onPressed});

  final UiUserId? userId;
  final double size;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    UiUserProfile? profile;
    try {
      profile = context.select(
        (UsersCubit cubit) => cubit.state.profile(userId: userId),
      );
    } on ProviderNotFoundException {
      profile = null;
    }

    final displayName = profile?.displayName ?? "";
    final image = profile?.profilePicture;
    final gradientKey = userId?.uuid ?? profile?.userId.uuid;

    return _Avatar(
      displayName: displayName,
      image: image,
      size: size,
      onPressed: onPressed,
      gradientKey: gradientKey,
    );
  }
}

class GroupAvatar extends StatelessWidget {
  const GroupAvatar({super.key, this.chatId, this.size = 24.0, this.onPressed});

  final ChatId? chatId;
  final double size;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    UiChatDetails? chat;
    try {
      chat = context.select((ChatDetailsCubit cubit) {
        final details = cubit.state.chat;
        if (chatId != null && details?.id != chatId) {
          return null;
        }
        return details;
      });
    } on ProviderNotFoundException {
      chat = null;
    }

    final displayName = chat?.title ?? chat?.displayName ?? "";
    final image = chat?.picture;
    final gradientKey = chatId?.uuid ?? chat?.id.uuid;

    return _Avatar(
      displayName: displayName,
      image: image,
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
    final foregroundImage = image != null
        ? CachedMemoryImage.fromImageData(image!)
        : null;
    final colors = CustomColorScheme.of(context);
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
                  color: colors.function.white,
                  fontSize: LabelFontSize.small2.size * size / 28,
                  fontWeight: FontWeight.w300,
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

  static const _start = 300;
  static const _end = 700;

  static final _gradients = [
    (AppColors.red[_start]!, AppColors.red[_end]!),
    (AppColors.orange[_start]!, AppColors.orange[_end]!),
    (AppColors.yellow[_start]!, AppColors.yellow[_end]!),
    (AppColors.green[_start]!, AppColors.green[_end]!),
    (AppColors.cyan[_start]!, AppColors.cyan[_end]!),
    (AppColors.blue[_start]!, AppColors.blue[_end]!),
    (AppColors.purple[_start]!, AppColors.purple[_end]!),
    (AppColors.magenta[_start]!, AppColors.magenta[_end]!),
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

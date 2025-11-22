// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

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
    final gradientColors = _gradientColors[_gradientIndexForUuid(gradientKey)];

    return GestureDetector(
      onTap: onPressed,
      child: MouseRegion(
        cursor: onPressed != null
            ? SystemMouseCursors.click
            : SystemMouseCursors.basic,
        child: SizedBox(
          width: size,
          height: size,
          child: DecoratedBox(
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              gradient: foregroundImage == null
                  ? LinearGradient(
                      colors: gradientColors,
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

const _gradients = [
  ('#FDD819', '#E80505'),
  ('#DAE2F8', '#D6A4A4'),
  ('#ED4264', '#ACA89E'),
  ('#24C6DC', '#514A9D'),
  ('#1CD8D2', '#93EDC7'),
  ('#9C2CFF', '#4389A2'),
  ('#134E5E', '#71B280'),
  ('#FF8008', '#FFE8AD'),
  ('#8F971D', '#93F9B9'),
  ('#95EB33', '#F45C43'),
  ('#AA076B', '#61045F'),
  ('#FFE259', '#ABFF51'),
  ('#5465D9', '#F8D365'),
  ('#6B8C56', '#282638'),
  ('#F9AE68', '#EC54C1'),
];

final _gradientColors = _gradients
    .map((pair) => [_hexToColor(pair.$1), _hexToColor(pair.$2)])
    .toList(growable: false);

Color _hexToColor(String hex) {
  final value = hex.replaceFirst('#', '');
  final buffer = value.length == 6 ? 'ff$value' : value;
  return Color(int.parse(buffer, radix: 16));
}

int _gradientIndexForUuid(UuidValue? uuid) {
  if (uuid == null) {
    return 0;
  }
  var hash = 0;
  for (final codeUnit in uuid.uuid.codeUnits) {
    hash = (hash * 31 + codeUnit) & 0x7fffffff;
  }
  return hash % _gradientColors.length;
}

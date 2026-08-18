// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/avatar/avatar.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/util/cached_memory_image.dart';
import 'package:flutter/widgets.dart';
import 'package:provider/provider.dart';
import 'package:uuid/uuid.dart';

class UserAvatar extends StatelessWidget {
  const UserAvatar({
    super.key,
    required this.profile,
    this.size = 24.0,
    this.onPressed,
  });

  final UiUserProfile profile;
  final double size;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return _Avatar(
      displayName: profile.displayName,
      image: profile.profilePicture,
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

/// Avatar for a chat the host already has.
class ChatAvatarView extends StatelessWidget {
  const ChatAvatarView({
    super.key,
    required this.chat,
    this.size = 24.0,
    this.onPressed,
  });

  final UiChatDetails chat;
  final double size;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final showImage = switch (chat.chatType) {
      UiChatType_Connection() || UiChatType_Group() => true,
      _ => false,
    };

    final displayName = chat.title;
    final image = chat.picture;
    final gradientKey = chat.userId?.uuid ?? chat.id.uuid;

    return _Avatar(
      displayName: displayName,
      image: showImage ? image : null,
      size: size,
      onPressed: onPressed,
      gradientKey: gradientKey,
    );
  }
}

/// Adapts the protocol's picture blobs and uuids to what [Avatar] paints.
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
    final image = this.image;
    // Decode straight to the circle's pixel size: profile pictures arrive far
    // larger than any avatar renders them.
    final targetSize = (size * MediaQuery.devicePixelRatioOf(context)).round();

    return Avatar(
      displayName: displayName,
      size: size,
      image: image != null
          ? CachedMemoryImage.fromImageData(
              image,
              targetWidth: targetSize,
              targetHeight: targetSize,
            )
          : null,
      gradientSeed: gradientKey?.uuid,
      onTap: onPressed,
    );
  }
}

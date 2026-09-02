// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/ds/patterns/contact_request_card/contact_request_card.dart';
import 'package:air/ds/patterns/contact_request_card/contact_request_card_tokens.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/util/cached_memory_image.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

sealed class ContactRequestSource {
  const ContactRequestSource();

  const factory ContactRequestSource.targetedMessage({
    required String originChatTitle,
  }) = _TargetedMessageContactRequest;

  const factory ContactRequestSource.username({required UiUsername username}) =
      _UsernameContactRequest;
}

class _TargetedMessageContactRequest extends ContactRequestSource {
  const _TargetedMessageContactRequest({required this.originChatTitle});

  final String originChatTitle;
}

class _UsernameContactRequest extends ContactRequestSource {
  const _UsernameContactRequest({required this.username});

  final UiUsername username;
}

class ContactRequestDialog extends HookWidget {
  const ContactRequestDialog({
    super.key,
    required this.sender,
    required this.source,
  });

  final UiUserId sender;
  final ContactRequestSource source;

  @override
  Widget build(BuildContext context) {
    final senderProfile = context.select(
      (UsersCubit c) => c.state.profile(userId: sender),
    );

    final loc = AppLocalizations.of(context);
    final tokens = ContactRequestCardTokens.current;

    // The accept runs as a persistent background job. The button stays
    // disabled as long as the job is pending. A permanently failed job
    // re-enables it: accepting again re-arms the job.
    final acceptStatus = context.select(
      (ChatDetailsCubit c) => c.state.chat?.connectionAccept,
    );
    final tapped = useState(false);
    final isAccepting =
        tapped.value || acceptStatus is ConnectionAcceptStatus_Pending;

    final failedReason = switch (acceptStatus) {
      ConnectionAcceptStatus_Failed(:final reason) => reason,
      _ => null,
    };
    useEffect(() {
      if (acceptStatus != null) {
        // The job state took over from the local tap feedback.
        tapped.value = false;
      }
      if (failedReason != null) {
        // The effect runs during build, but showing a banner rebuilds the
        // messenger, so defer it to after the frame.
        WidgetsBinding.instance.addPostFrameCallback((_) {
          Logger.detached(
            "ContactRequestDialog",
          ).severe("Failed to accept contact request: $failedReason");
          showErrorBannerStandalone(
            (loc) => loc.contactRequestDialog_error_fatal,
          );
        });
      }
      return null;
    }, [acceptStatus]);

    final subtitle = switch (source) {
      _TargetedMessageContactRequest(:final originChatTitle) =>
        loc.systemMessage_receivedDirectConnectionRequest(
          senderProfile.displayName,
          originChatTitle,
        ),
      _UsernameContactRequest(:final username) =>
        loc.systemMessage_receivedHandleConnectionRequest(
          senderProfile.displayName,
          username.plaintext,
        ),
    };

    return ContactRequestCard(
      tokens: tokens,
      title: loc.contactRequestDialog_title,
      subtitle: subtitle,
      displayName: senderProfile.displayName,
      gradientSeed: senderProfile.userId.uuid.uuid,
      image: _picture(
        context,
        senderProfile.profilePicture,
        ContactRequestCardTokens.avatarSize,
      ),
      pictureRevealLabel: loc.contactRequestDialog_avatarHint,
      acceptLabel: loc.contactRequestDialog_confirm,
      dismissLabel: loc.contactRequestDialog_cancel,
      onAccept: () => _accept(context, tapped),
      onDismiss: () => context.read<NavigationCubit>().closeChat(),
      isAccepting: isAccepting,
    );
  }

  ImageProvider? _picture(
    BuildContext context,
    ImageData? picture,
    double size,
  ) {
    if (picture == null) return null;
    // Decode straight to the circle's pixel size: profile pictures arrive far
    // larger than any avatar renders them.
    final targetSize = (size * MediaQuery.devicePixelRatioOf(context)).round();
    return CachedMemoryImage.fromImageData(
      picture,
      targetWidth: targetSize,
      targetHeight: targetSize,
    );
  }

  void _accept(BuildContext context, ValueNotifier<bool> tapped) async {
    tapped.value = true;

    final chatDetailsCubit = context.read<ChatDetailsCubit>();
    try {
      await chatDetailsCubit.acceptContactRequest();
    } catch (e, stackTrace) {
      Logger.detached(
        "ContactRequestDialog",
      ).severe("Failed to accept contact request: $e", e, stackTrace);
      showErrorBannerStandalone((loc) => loc.contactRequestDialog_error_fatal);
      tapped.value = false;
    }
  }
}

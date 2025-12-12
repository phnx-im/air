// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/chat/chat_details.dart';
import 'package:air/core/api/types.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/main.dart';
import 'package:air/navigation/navigation.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/button/button.dart';
import 'package:air/ui/components/modal/app_dialog.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:air/widgets/avatar.dart' show UserAvatar;
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

sealed class ContactRequestSource {
  const ContactRequestSource();

  const factory ContactRequestSource.targetedMessage({
    required String originChatTitle,
  }) = _TargetedMessageContactRequest;

  const factory ContactRequestSource.handle({required UiUserHandle handle}) =
      _HandleContactRequest;
}

class _TargetedMessageContactRequest extends ContactRequestSource {
  const _TargetedMessageContactRequest({required this.originChatTitle});

  final String originChatTitle;
}

class _HandleContactRequest extends ContactRequestSource {
  const _HandleContactRequest({required this.handle});

  final UiUserHandle handle;
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

    final colors = CustomColorScheme.of(context);
    final loc = AppLocalizations.of(context);

    final showImage = useState(false);

    final message = switch (source) {
      _TargetedMessageContactRequest(:final originChatTitle) =>
        loc.systemMessage_receivedDirectConnectionRequest(
          senderProfile.displayName,
          originChatTitle,
        ),
      _HandleContactRequest(:final handle) =>
        loc.systemMessage_receivedHandleConnectionRequest(
          senderProfile.displayName,
          handle.plaintext,
        ),
    };

    return AppDialogContainer(
      backgroundColor: colors.backgroundBase.secondary,
      maxWidth: 360,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            loc.contactRequestDialog_title,
            style: TextStyle(
              fontSize: HeaderFontSize.h4.size,
              fontWeight: FontWeight.bold,
            ),
          ),

          const SizedBox(height: Spacings.l),

          InkWell(
            onTap: () {
              showImage.value = !showImage.value;
            },
            child: UserAvatar(
              profile: senderProfile,
              size: 96,
              showInitials: senderProfile.profilePicture == null,
              showImage: showImage.value,
            ),
          ),

          if (senderProfile.profilePicture != null) ...[
            const SizedBox(height: Spacings.xxs),
            Text(
              loc.contactRequestDialog_avatarHint,
              style: TextStyle(
                fontSize: LabelFontSize.small2.size,
                color: colors.text.tertiary,
              ),
            ),
          ],

          const SizedBox(height: Spacings.l),

          Text(
            message,
            style: TextStyle(
              fontSize: BodyFontSize.base.size,
              color: colors.text.secondary,
            ),
            textAlign: .center,
          ),

          const SizedBox(height: Spacings.l),

          Row(
            children: [
              Expanded(
                child: AppButton(
                  onPressed: () {
                    context.read<NavigationCubit>().closeChat();
                  },
                  type: .secondary,
                  label: loc.contactRequestDialog_cancel,
                ),
              ),
              const SizedBox(width: Spacings.xs),
              const Expanded(child: _AcceptButton()),
            ],
          ),
        ],
      ),
    );
  }
}

class _AcceptButton extends HookWidget {
  const _AcceptButton();

  @override
  Widget build(BuildContext context) {
    final isAccepting = useState(false);
    final loc = AppLocalizations.of(context);
    return AppButton(
      onPressed: () => _onPressed(context, isAccepting),
      type: .primary,
      state: isAccepting.value ? AppButtonState.pending : AppButtonState.active,
      label: loc.contactRequestDialog_confirm,
    );
  }

  void _onPressed(BuildContext context, ValueNotifier<bool> isAccepting) async {
    isAccepting.value = true;

    final chatDetailsCubit = context.read<ChatDetailsCubit>();
    try {
      await chatDetailsCubit.acceptContactRequest();
    } catch (e, stackTrace) {
      Logger.detached(
        "ContactRequestDialog",
      ).severe("Failed to accept contact request {e}", e, stackTrace);
      showErrorBannerStandalone((loc) => loc.contactRequestDialog_error_fatal);
    } finally {
      isAccepting.value = false;
    }
  }
}

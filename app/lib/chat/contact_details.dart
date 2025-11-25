// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/modal/app_dialog.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:air/widgets/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:provider/provider.dart';

import 'block_contact_button.dart';
import 'delete_contact_button.dart';
import 'report_spam_button.dart';
import 'unblock_contact_button.dart';

class ContactDetailsView extends StatelessWidget {
  const ContactDetailsView({
    super.key,
    required this.profile,
    this.contactChatId,
    this.isBlocked = false,
    this.groupTitle,
  });

  final UiUserProfile profile;
  final ChatId? contactChatId;
  final bool isBlocked;
  final String? groupTitle;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Align(
      alignment: Alignment.topCenter,
      child: Column(
        children: [
          const Spacer(),

          UserAvatar(
            size: 192,
            displayName: profile.displayName,
            image: profile.profilePicture,
          ),

          const SizedBox(height: Spacings.m),
          OutlinedButton(
            onPressed: () => _handleChat(context),
            style: const ButtonStyle(
              minimumSize: WidgetStatePropertyAll(Size(192 * 2 / 3, 0)),
            ),
            child: Text(
              "Chat",
              style: TextStyle(fontSize: LabelFontSize.base.size),
            ),
          ),

          const Spacer(),

          ReportSpamButton(userId: profile.userId),

          const SizedBox(height: Spacings.s),
          isBlocked
              ? UnblockContactButton(
                  userId: profile.userId,
                  displayName: profile.displayName,
                )
              : BlockContactButton(
                  userId: profile.userId,
                  displayName: profile.displayName,
                ),

          if (contactChatId case final chatId?) ...[
            const SizedBox(height: Spacings.s),
            DeleteContactButton(
              chatId: chatId,
              displayName: profile.displayName,
            ),
          ],
        ],
      ),
    );
  }

  void _handleChat(BuildContext context) async {
    final contact = await context.read<UserCubit>().contact(
      userId: profile.userId,
    );

    // No contact found means we can establish a new connection
    if (contact == null && groupTitle != null) {
      if (!context.mounted) return;
      showDialog(
        context: context,
        builder: (context) => _AddAirContactDialog(
          displayName: profile.displayName,
          groupTitle: groupTitle!,
        ),
      );
    }
  }
}

class _AddAirContactDialog extends HookWidget {
  const _AddAirContactDialog({
    required this.displayName,
    required this.groupTitle,
  });

  final String displayName;
  final String groupTitle;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    final colors = CustomColorScheme.of(context);

    return AppDialog(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Center(
            child: Text(
              loc.addAirContactDialog_title,
              style: TextStyle(
                fontSize: HeaderFontSize.h4.size,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),

          const SizedBox(height: Spacings.xxs),

          Text(
            loc.addAirContactDialog_content(displayName, groupTitle),
            style: TextStyle(
              color: colors.text.secondary,
              fontSize: BodyFontSize.base.size,
            ),
          ),

          const SizedBox(height: Spacings.m),

          Row(
            children: [
              Expanded(
                child: OutlinedButton(
                  onPressed: () {
                    Navigator.of(context).pop(false);
                  },
                  child: Text(
                    loc.addAirContactDialog_cancel,
                    style: TextStyle(fontSize: LabelFontSize.base.size),
                  ),
                ),
              ),

              const SizedBox(width: Spacings.xs),

              Expanded(
                child: OutlinedButton(
                  onPressed: () => _handleSendChatRequest(context),
                  style: ButtonStyle(
                    backgroundColor: WidgetStatePropertyAll(
                      colors.accent.primary,
                    ),
                    overlayColor: WidgetStatePropertyAll(colors.accent.primary),
                    foregroundColor: WidgetStatePropertyAll(
                      colors.function.toggleWhite,
                    ),
                  ),
                  child: Text(
                    loc.addAirContactDialog_confirm,
                    style: TextStyle(fontSize: LabelFontSize.base.size),
                  ),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }

  void _handleSendChatRequest(BuildContext context) async {
    // TODO: Implement
  }
}

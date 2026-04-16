// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/app_scaffold.dart';
import 'package:air/ui/components/button/button.dart';
import 'package:air/ui/icons/icons.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:flutter/material.dart';

// Placeholder invite codes for display
const _inviteCodes = [
  'CATSRULE',
  'LOVEDOGS',
  'OMGRATSS',
  'PANDAWOW',
  'WOOFMEOW',
  'FLEXPOST',
];

class InviteCodesScreen extends StatelessWidget {
  const InviteCodesScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return AppScaffold(
      title: 'Invite codes',
      backgroundColor: colors.backgroundBase.secondary,
      child: Align(
        alignment: Alignment.topCenter,
        child: Container(
          constraints: isPointer() ? const BoxConstraints(maxWidth: 800) : null,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const SizedBox(height: Spacings.s),
                const _InviteCodesList(),
                const SizedBox(height: Spacings.m),
                Row(
                  children: [
                    const Spacer(),
                    IntrinsicWidth(
                      child: AppButton(
                        size: AppButtonSize.small,
                        type: AppButtonType.secondary,
                        label: 'Copy all',
                        onPressed: () {},
                      ),
                    ),
                    const Spacer(),
                  ],
                ),
                const SizedBox(height: Spacings.m),
                const _InfoText(),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _InviteCodesList extends StatelessWidget {
  const _InviteCodesList();

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Container(
      decoration: BoxDecoration(
        color: colors.backgroundElevated.primary,
        borderRadius: BorderRadius.circular(Spacings.s),
      ),
      child: Column(
        children: _inviteCodes
            .expand(
              (code) => [
                _InviteCodeItem(code: code),
                if (code != _inviteCodes.last)
                  Divider(
                    height: 1,
                    thickness: 1,
                    color: colors.separator.primary,
                  ),
              ],
            )
            .toList(),
      ),
    );
  }
}

class _InviteCodeItem extends StatelessWidget {
  const _InviteCodeItem({required this.code});

  final String code;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: Spacings.s,
        vertical: Spacings.xs,
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(
              code,
              style: TextStyle(
                fontSize: BodyFontSize.base.size,
                color: colors.text.primary,
              ),
            ),
          ),
          const SizedBox(width: Spacings.xs),
          InkWell(
            onTap: () {},
            mouseCursor: SystemMouseCursors.click,
            borderRadius: BorderRadius.circular(Spacings.xxs),
            child: AppIcon.copy(size: 24, color: colors.text.tertiary),
          ),
        ],
      ),
    );
  }
}

class _InfoText extends StatelessWidget {
  const _InfoText();

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    final style = TextStyle(
      fontSize: BodyFontSize.small1.size,
      color: colors.text.quaternary,
    );

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: Spacings.xxs),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Air is in a limited access phase. Everyone who wants to join needs an invite code.',
            style: style,
          ),
          const SizedBox(height: Spacings.xs),
          Text(
            'Share these codes with your friends or anyone else who wants to join Air! New codes will be added periodically.',
            style: style,
          ),
        ],
      ),
    );
  }
}

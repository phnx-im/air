// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/theme/theme.dart';
import 'package:air/ui/colors/themes.dart';
import 'package:air/ui/components/app_scaffold.dart';
import 'package:air/ui/components/button/button.dart';
import 'package:air/ui/icons/icons.dart';
import 'package:air/ui/typography/font_size.dart';
import 'package:air/user/user.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class InvitationCodesScreen extends StatelessWidget {
  const InvitationCodesScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return BlocProvider<InvitationCodesCubit>(
      create: (BuildContext context) {
        final userCubit = context.read<UserCubit>();
        return InvitationCodesCubit(userCubit: userCubit);
      },
      child: const InvitationCodesView(),
    );
  }
}

class InvitationCodesView extends StatelessWidget {
  const InvitationCodesView({super.key});

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
                const _InvitationCodesList(),
                const SizedBox(height: Spacings.m),
                Row(
                  children: [
                    const Spacer(),
                    AppButton(
                      size: AppButtonSize.small,
                      type: AppButtonType.secondary,
                      label: 'Copy all',
                      onPressed: () {},
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

class _InvitationCodesList extends StatelessWidget {
  const _InvitationCodesList();

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    final invitationCodes = context.select(
      (InvitationCodesCubit cubit) => cubit.state.codes,
    );
    // final invitationCodes = _inviteCodes;

    return Container(
      decoration: BoxDecoration(
        color: colors.backgroundElevated.primary,
        borderRadius: BorderRadius.circular(Spacings.s),
      ),
      child: Column(
        children: invitationCodes.isEmpty
            ? [const _InvitationCodeEmptyItem()]
            : invitationCodes
                  .expand(
                    (code) => [
                      switch (code) {
                        UiInvitationCode_Code(field0: final code) =>
                          _InvitationCodeItem(code: code),
                        UiInvitationCode_Token(field0: final token) =>
                          _InvitationCodeUnlockButton(tokenId: token),
                      },
                      if (code != invitationCodes.last)
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

class _InvitationCodeItem extends StatelessWidget {
  const _InvitationCodeItem({required this.code});

  final InvitationCode code;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return InkWell(
      onTap: () {
        context.read<InvitationCodesCubit>().markInvitationCodeAsCopied(
          copiedCode: code.code,
        );
      },
      mouseCursor: SystemMouseCursors.click,
      borderRadius: BorderRadius.circular(Spacings.s),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Spacings.s,
          vertical: Spacings.xs,
        ),
        child: Row(
          children: [
            Expanded(
              child: Text(
                code.code,
                style: TextStyle(
                  fontSize: BodyFontSize.base.size,
                  color: colors.text.primary,
                  decoration: .lineThrough,
                ),
              ),
            ),
            const SizedBox(width: Spacings.xs),
            AppIcon.copy(size: 24, color: colors.text.tertiary),
          ],
        ),
      ),
    );
  }
}

class _InvitationCodeUnlockButton extends StatelessWidget {
  const _InvitationCodeUnlockButton({required this.tokenId});

  final TokenId tokenId;

  @override
  Widget build(BuildContext context) {
    final colors = CustomColorScheme.of(context);

    return InkWell(
      onTap: () {
        context.read<InvitationCodesCubit>().requestInvitationCode(
          tokenId: tokenId,
        );
      },
      mouseCursor: SystemMouseCursors.click,
      borderRadius: BorderRadius.circular(Spacings.s),
      child: Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: Spacings.s,
          vertical: Spacings.xs,
        ),
        child: Row(
          children: [
            Text(
              "Tap to unlock",
              style: TextStyle(
                fontSize: BodyFontSize.base.size,
                fontStyle: FontStyle.italic,
                color: colors.text.tertiary,
              ),
            ),
            const Spacer(),
            AppIcon.circleDashed(size: 24, color: colors.text.tertiary),
          ],
        ),
      ),
    );
  }
}

class _InvitationCodeEmptyItem extends StatelessWidget {
  const _InvitationCodeEmptyItem();

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
              "No invitation codes available",
              style: TextStyle(
                fontSize: BodyFontSize.base.size,
                color: colors.text.tertiary,
              ),
            ),
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

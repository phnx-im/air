// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/scaffold/app_scaffold.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/features/you/invitation_codes_cubit.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class InvitationCodesScreen extends StatelessWidget {
  const InvitationCodesScreen({super.key, this.invitationCodesCubit});

  final InvitationCodesCubit? invitationCodesCubit;

  @override
  Widget build(BuildContext context) {
    return invitationCodesCubit != null
        ? BlocProvider<InvitationCodesCubit>.value(
            value: invitationCodesCubit!,
            child: const InvitationCodesView(),
          )
        : BlocProvider<InvitationCodesCubit>(
            create: (BuildContext context) =>
                invitationCodesCubit ??
                InvitationCodesCubit(userCubit: context.read<UserCubit>()),
            child: const InvitationCodesView(),
          );
  }
}

class InvitationCodesView extends StatelessWidget {
  const InvitationCodesView({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    return AppScaffold(
      title: loc.invitationCodesScreen_title,
      backgroundColor: palette.backgroundBase.secondary,
      child: Align(
        alignment: Alignment.topCenter,
        child: Container(
          constraints: DeviceType.isDesktop
              ? const BoxConstraints(maxWidth: 800)
              : null,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const SizedBox(height: S.s16),
                const _InvitationCodesList(),
                const SizedBox(height: S.s24),
                Row(
                  children: [
                    const Spacer(),

                    Builder(
                      builder: (context) {
                        final anyUncopiedCode = context.select(
                          (InvitationCodesCubit cubit) => cubit.state.codes
                              .whereType<UiInvitationCode_Code>()
                              .any((code) => !code.field0.copied),
                        );
                        return Button(
                          size: ButtonSize.small,
                          type: ButtonType.secondary,
                          label: loc.invitationCodesScreen_copyAll,
                          state: anyUncopiedCode ? .active : .disabled,
                          onPressed: () => _handleCopyAll(context),
                        );
                      },
                    ),
                    const Spacer(),
                  ],
                ),
                const SizedBox(height: S.s16),
                Row(
                  children: [
                    const Spacer(),
                    Builder(
                      builder: (context) {
                        final anyCopiedCode = context.select(
                          (InvitationCodesCubit cubit) => cubit.state.codes
                              .whereType<UiInvitationCode_Code>()
                              .any((code) => code.field0.copied),
                        );
                        return Button(
                          size: ButtonSize.small,
                          type: ButtonType.secondary,
                          label: loc.invitationCodesScreen_removeUsedCodes,
                          state: anyCopiedCode ? .active : .disabled,
                          onPressed: () => _handleClearCopied(context),
                        );
                      },
                    ),
                    const Spacer(),
                  ],
                ),
                const SizedBox(height: S.s24),
                const _InfoText(),
              ],
            ),
          ),
        ),
      ),
    );
  }

  void _handleCopyAll(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final invitationCodesCubit = context.read<InvitationCodesCubit>();
    final codes = invitationCodesCubit.state.codes
        .whereType<UiInvitationCode_Code>()
        .where((code) => !code.field0.copied)
        .map((code) => code.field0.code)
        .toList();

    Clipboard.setData(
      ClipboardData(
        text: loc.invitationCodesScreen_codesClipboardMessage(codes.join("\n")),
      ),
    );

    for (final code in codes) {
      invitationCodesCubit.markInvitationCodeAsCopied(copiedCode: code);
    }

    showSnackBarStandalone(
      (loc) =>
          SnackBar(content: Text(loc.invitationCodesScreen_copiedToClipboard)),
    );
  }

  void _handleClearCopied(BuildContext context) {
    context.read<InvitationCodesCubit>().clearCopiedCodes();
  }
}

class _InvitationCodesList extends StatelessWidget {
  const _InvitationCodesList();

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    final invitationCodes = context.select(
      (InvitationCodesCubit cubit) => cubit.state.codes,
    );

    return Container(
      decoration: BoxDecoration(
        color: palette.backgroundElevated.primary,
        borderRadius: BorderRadius.circular(CornerRadius.px16),
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
                          _InvitationTokenItem(tokenId: token),
                      },
                      if (code != invitationCodes.last)
                        Divider(
                          height: 1,
                          thickness: StrokeWidth.px1,
                          color: palette.separator.primary,
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
    final palette = SemanticPalette.of(context);

    return InkWell(
      onTap: () => _handleCopy(context),
      mouseCursor: SystemMouseCursors.click,
      borderRadius: BorderRadius.circular(CornerRadius.px16),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s12),
        child: Row(
          children: [
            Expanded(
              child: Text(
                code.code,
                style: typeScale.body.regular
                    .style(color: palette.text.primary)
                    .copyWith(
                      decoration: code.copied
                          ? TextDecoration.lineThrough
                          : null,
                    ),
              ),
            ),
            const SizedBox(width: S.s12),
            AppIcon.copy(size: 24, color: palette.text.tertiary),
          ],
        ),
      ),
    );
  }

  void _handleCopy(BuildContext context) {
    final loc = AppLocalizations.of(context);

    Clipboard.setData(
      ClipboardData(
        text: loc.invitationCodesScreen_codeClipboardMessage(code.code),
      ),
    );

    showSnackBarStandalone(
      (loc) =>
          SnackBar(content: Text(loc.invitationCodesScreen_copiedToClipboard)),
    );

    if (!code.copied) {
      final invitationCodesCubit = context.read<InvitationCodesCubit>();
      invitationCodesCubit.markInvitationCodeAsCopied(copiedCode: code.code);
    }
  }
}

class _InvitationTokenItem extends StatelessWidget {
  const _InvitationTokenItem({required this.tokenId});

  final TokenId tokenId;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return InkWell(
      onTap: () => _handleUnlock(context),
      mouseCursor: SystemMouseCursors.click,
      borderRadius: BorderRadius.circular(CornerRadius.px16),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s12),
        child: Row(
          children: [
            Text(
              AppLocalizations.of(context).invitationCodesScreen_tapToGetCode,
              style: typeScale.body.regular
                  .style(color: palette.text.tertiary)
                  .copyWith(fontStyle: FontStyle.italic),
            ),
            const Spacer(),
            AppIcon.circleDashed(size: 24, color: palette.text.tertiary),
          ],
        ),
      ),
    );
  }

  void _handleUnlock(BuildContext context) async {
    try {
      final error = await context
          .read<InvitationCodesCubit>()
          .requestInvitationCode(tokenId: tokenId);
      switch (error) {
        case RequestInvitationCodeError.globalQuotaExceeded:
          showSnackBarStandalone(
            (loc) => SnackBar(
              content: Text(loc.invitationCodesScreen_global_quota_exceeded),
            ),
          );
          break;
        case null:
          return;
      }
    } catch (e) {
      showErrorBannerStandalone(
        (loc) => loc.invitationCodesScreen_errorRequestingCode,
      );
    }
  }
}

class _InvitationCodeEmptyItem extends StatelessWidget {
  const _InvitationCodeEmptyItem();

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s12),
      child: Row(
        children: [
          Expanded(
            child: Text(
              AppLocalizations.of(context).invitationCodesScreen_empty,
              style: typeScale.body.regular.style(color: palette.text.tertiary),
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
    final palette = SemanticPalette.of(context);

    final style = typeScale.body.s.style(color: palette.text.quaternary);

    final loc = AppLocalizations.of(context);

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: S.s8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(loc.invitationCodesScreen_infoText1, style: style),
          const SizedBox(height: S.s12),
          Text(loc.invitationCodesScreen_infoText2, style: style),
        ],
      ),
    );
  }
}

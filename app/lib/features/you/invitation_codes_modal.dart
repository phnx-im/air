// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/components/list_group/list_group.dart';
import 'package:air/ds/components/list_group/list_group_tokens.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_route.dart';
import 'package:air/features/you/invitation_codes_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Opens the invite codes on the surface the device calls for.
Future<void> showInvitationCodes(BuildContext context) {
  // The cubit lives below the navigator the modal opens on, so the modal
  // carries it along.
  final cubit = context.read<InvitationCodesCubit>();

  return showAppModal<void>(
    context: context,
    builder: (_) => BlocProvider<InvitationCodesCubit>.value(
      value: cubit,
      child: const InvitationCodesModal(),
    ),
  );
}

/// The invite codes, what to do with them, and what they are for.
class InvitationCodesModal extends StatelessWidget {
  const InvitationCodesModal({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return ModalScaffold(
      title: loc.invitationCodesScreen_title,
      onDismiss: () => Navigator.of(context).pop(),
      child: const ModalBody(top: S.s16, child: InvitationCodesContent()),
    );
  }
}

/// The codes list and its actions. Sized and scrolled by its host.
class InvitationCodesContent extends StatelessWidget {
  const InvitationCodesContent({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);

    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .start,
      children: [
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

    // The group's own fill, so the codes read against either modal surface.
    return ListGroup(
      tokens: ListGroupTokens.current,
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
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s12),
        child: Row(
          children: [
            Text(
              AppLocalizations.of(context).invitationCodesScreen_tapToGetCode,
              style: typeScale.body.regular
                  .style(color: palette.text.tertiary)
                  .copyWith(fontStyle: .italic),
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
        crossAxisAlignment: .start,
        children: [
          Text(loc.invitationCodesScreen_infoText1, style: style),
          const SizedBox(height: S.s12),
          Text(loc.invitationCodesScreen_infoText2, style: style),
        ],
      ),
    );
  }
}

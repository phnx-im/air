// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/list_row/list_row.dart';
import 'package:air/ds/components/list_row/list_row_tokens.dart';
import 'package:air/ds/components/toggle/toggle.dart';
import 'package:air/ds/components/toggle/toggle_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/features/user/contact_verification.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/user/users_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:air/features/user/avatar.dart';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';

final _log = Logger('SafetyCodePane');

/// The safety code for one profile.
class SafetyCodePane extends StatelessWidget {
  const SafetyCodePane({super.key, required this.user});

  final UiUserId user;

  @override
  Widget build(BuildContext context) {
    final profile = context.select(
      (UsersCubit cubit) => cubit.state.profile(userId: user),
    );
    final loc = AppLocalizations.of(context);

    return ModalPane(
      title: loc.safetyCodeScreen_title,
      child: SafetyCodeView(profile: profile),
    );
  }
}

class SafetyCodeView extends StatelessWidget {
  const SafetyCodeView({super.key, required this.profile});

  final UiUserProfile profile;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    return ModalBody(
      top: S.s12,
      child: Column(
        children: [
          UserAvatar(profile: profile, size: 192),

          const SizedBox(height: S.s16),

          Text(
            profile.displayName,
            style: typeScale.header.xl.style(weight: Weight.emphasized),
          ),

          const SizedBox(height: S.s4),

          ContactVerificationLabel(userId: profile.userId),

          const SizedBox(height: S.s24),

          _SafetyCode(userId: profile.userId),

          const SizedBox(height: S.s24),

          Text(
            style: typeScale.body.s.style(color: palette.text.tertiary),
            loc.safetyCodeScreen_safetyCodeExplanation(profile.displayName),
          ),
        ],
      ),
    );
  }
}

class _SafetyCode extends HookWidget {
  const _SafetyCode({required this.userId});

  final UiUserId userId;

  @override
  Widget build(BuildContext context) {
    final Future<intArray12> safetyCodeFut = useMemoized(
      () => context.read<UserCubit>().safetyCodes(userId),
      [userId],
    );
    final safetyCode = useFuture(safetyCodeFut);
    final (p1, p2, p3) = safetyCode.data?.paragraphs ?? ('', '', '');

    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    final codeStyle = typeScale.body.s
        .style(
          color: safetyCode.hasData
              ? palette.text.primary
              : palette.text.tertiary,
        )
        .withSystemMonospace()
        .copyWith(height: 1.5);

    final card = InkWell(
      onTap: safetyCode.hasData
          ? () {
              Clipboard.setData(
                ClipboardData(text: safetyCode.data!.textRepresentation),
              );
              showSnackBarStandalone(
                (loc) => loc.safetyCodeScreen_copiedToClipboard,
                tone: .success,
              );
            }
          : null,
      child: Container(
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(CornerRadius.px12),
          color: palette.backgroundBase.secondary,
        ),
        padding: const EdgeInsets.symmetric(vertical: S.s16, horizontal: S.s24),
        child: Column(
          children: [
            Text(p1, style: codeStyle),
            Text(p2, style: codeStyle),
            Text(p3, style: codeStyle),
            const SizedBox(height: S.s24),
            Row(
              mainAxisSize: .min,
              mainAxisAlignment: .center,
              children: [
                AppIcon.copy(color: palette.text.tertiary, size: 16),
                const SizedBox(width: S.s8),
                Text(
                  DeviceType.isDesktop
                      ? loc.safetyCodeScreen_copyHint_desktop
                      : loc.safetyCodeScreen_copyHint,
                  style: typeScale.body.s.style(color: palette.text.tertiary),
                ),
              ],
            ),
          ],
        ),
      ),
    );

    return Column(
      crossAxisAlignment: .stretch,
      children: [
        card,
        // A user can only be marked once their credential is known, which
        // the loaded code proves.
        if (safetyCode.hasData) ...[
          const SizedBox(height: S.s16),
          _MarkAsVerified(userId: userId),
        ],
      ],
    );
  }
}

/// Toggle row that marks the user as verified, in the code card's fill so the
/// two read as one block.
class _MarkAsVerified extends StatelessWidget {
  const _MarkAsVerified({required this.userId});

  final UiUserId userId;

  @override
  Widget build(BuildContext context) {
    final verified = context.select(
      (UsersCubit cubit) =>
          cubit.state.verification(userId: userId) ==
          UiContactVerification.verified,
    );
    final loc = AppLocalizations.of(context);
    final palette = SemanticPalette.of(context);

    void setVerified(bool value) async {
      try {
        await context.read<UserCubit>().setContactVerified(userId, value);
      } catch (error) {
        _log.severe('Failed to change the verification: $error');
      }
    }

    return ListRow(
      tokens: ListRowTokens.current,
      label: loc.safetyCodeScreen_markAsVerified,
      fill: palette.backgroundBase.secondary,
      trailing: Toggle(
        tokens: ToggleTokens.current,
        value: verified,
        onChanged: setVerified,
      ),
      onTap: () => setVerified(!verified),
    );
  }
}

extension on intArray12 {
  String get textRepresentation {
    final (p1, p2, p3) = paragraphs;
    return [p1, p2, p3].join('\n');
  }

  (String, String, String) get paragraphs {
    String sliceToString(List<int> slice) => slice
        .map((i) => i.toString().padLeft(5, '0'))
        .slices(2)
        .map((slice) => slice.join(' '))
        .join(' ');
    return (
      sliceToString(inner.sublist(0, 4)),
      sliceToString(inner.sublist(4, 8)),
      sliceToString(inner.sublist(8, 12)),
    );
  }
}

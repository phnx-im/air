// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core_extension.dart';
import 'package:air/ds/components/button_cta/button_cta.dart';
import 'package:air/ds/components/button_cta/button_cta_tokens.dart';
import 'package:air/ds/components/list_row/list_row.dart';
import 'package:air/ds/components/list_row/list_row_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/mute_chat_sheet.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Shape the mute toggle takes, picked by the surface hosting it.
enum MuteButtonShape {
  /// Circle with a label under it, for the row of actions a profile leads
  /// with.
  cta,

  /// Full-width row, for a surface whose actions read as a list.
  row,
}

/// Mute / unmute for the open chat. The state and the toggle are the same
/// whichever shape it takes, so the shape is a parameter rather than a second
/// widget carrying a copy of the logic.
class MuteButton extends StatelessWidget {
  const MuteButton({super.key, this.shape = MuteButtonShape.cta});

  final MuteButtonShape shape;

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final isMuted = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.isMuted ?? false,
    );

    final label = isMuted
        ? loc.contactDetailsScreen_unmute
        : loc.contactDetailsScreen_mute;
    final icon = isMuted ? AppIconType.bell : AppIconType.bellOff;

    final cubit = context.read<ChatDetailsCubit>();

    void toggle() => isMuted
        ? cubit.unmuteChat()
        : showMuteChatSheet(
            context,
            onMute: ({until}) {
              if (until != null) {
                cubit.muteChat(mutedUntil: until);
              } else {
                cubit.unmuteChat();
              }
            },
          );

    return switch (shape) {
      MuteButtonShape.cta => ButtonCTA(
        tokens: ButtonCTATokens.current,
        label: label,
        icon: icon,
        type: ButtonCTAType.secondary,
        onPressed: toggle,
      ),
      MuteButtonShape.row => ListRow(
        tokens: ListRowTokens.current,
        label: label,
        leading: AppIcon(type: icon, size: S.s24),
        fill: SemanticPalette.of(context).backgroundBase.secondary,
        radius: CornerRadius.px12,
        onTap: toggle,
      ),
    };
  }
}

// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core_extension.dart';
import 'package:air/ds/components/button_cta/button_cta.dart';
import 'package:air/ds/components/button_cta/button_cta_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/mute_chat_sheet.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Mute / unmute toggle for the open chat.
class MuteButton extends StatelessWidget {
  const MuteButton({super.key});

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

    void toggle() => isMuted
        ? context.read<ChatDetailsCubit>().unmuteChat()
        : showMuteChatSheet(context);

    return ButtonCTA(
      tokens: ButtonCTATokens.current,
      label: label,
      icon: icon,
      type: ButtonCTAType.secondary,
      onPressed: toggle,
    );
  }
}

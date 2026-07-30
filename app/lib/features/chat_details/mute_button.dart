// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/features/chat/mute_chat_sheet.dart';
import 'package:air/core/core_extension.dart';
import 'package:air/ds/foundations/type_scale.dart';
import 'package:air/ds/foundations/icons.dart';
import 'package:air/ds/foundations/dimensions.dart';
import 'package:air/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

class MuteButton extends StatelessWidget {
  const MuteButton({super.key});

  @override
  Widget build(BuildContext context) {
    final loc = AppLocalizations.of(context);
    final isMuted = context.select(
      (ChatDetailsCubit cubit) => cubit.state.chat?.isMuted ?? false,
    );
    return OutlinedButton(
      onPressed: () => isMuted
          ? context.read<ChatDetailsCubit>().unmuteChat()
          : showMuteChatSheet(context),
      style: const ButtonStyle(
        visualDensity: VisualDensity.compact,
        minimumSize: WidgetStatePropertyAll(Size(82, 32)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          isMuted
              ? const AppIcon.bell(size: 16)
              : const AppIcon.bellOff(size: 16),
          const SizedBox(width: S.s8),
          Text(
            isMuted
                ? loc.contactDetailsScreen_unmute
                : loc.contactDetailsScreen_mute,
            style: typeScale.body.regular.style(),
          ),
        ],
      ),
    );
  }
}

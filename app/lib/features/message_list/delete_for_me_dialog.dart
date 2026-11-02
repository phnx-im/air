// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/features/chat/chat_details_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/platform/haptics.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

/// Asks before deleting the message.
void showDeleteForMeDialog({
  required BuildContext context,
  required MessageId messageId,
}) {
  final loc = AppLocalizations.of(context);
  final cubit = context.read<ChatDetailsCubit>();

  showAdaptiveModal(
    context: context,
    builder: (modalContext) => AdaptiveDialogContent(
      title: loc.deleteMessageForMeDialog_title,
      description: loc.deleteMessageForMeDialog_description,
      primaryActionText: loc.deleteMessageForMeDialog_delete,
      onPrimaryAction: (_) {
        AppHaptics.destructive();
        cubit.deleteMessage(messageId: messageId, deleteMode: DeleteMode.forMe);
      },
      primaryType: ButtonType.secondary,
      primaryTone: ButtonTone.danger,
    ),
  );
}

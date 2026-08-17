// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart' show AppLocalizations;
import 'package:air/ds/patterns/message_separator/message_separator.dart';
import 'package:flutter/widgets.dart';

class UnreadDivider extends StatelessWidget {
  const UnreadDivider({super.key});

  @override
  Widget build(BuildContext context) => MessageSeparator(
    label: AppLocalizations.of(context).messageList_newMessages,
    variant: MessageSeparatorVariant.unread,
  );
}

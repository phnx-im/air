// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/toggle/toggle.dart';
import 'package:air/ds/components/toggle/toggle_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/material.dart';

/// A labeled row with a toggle, used for a single on/off setting.
class SwitchField extends StatelessWidget {
  const SwitchField({
    super.key,
    required this.onChanged,
    required this.value,
    required this.label,
  });

  final ValueChanged<bool> onChanged;
  final bool value;
  final String label;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return InkWell(
      onTap: () => onChanged(!value),
      child: Container(
        decoration: BoxDecoration(
          color: palette.backgroundBase.secondary,
          borderRadius: BorderRadius.circular(CornerRadius.px16),
        ),
        padding: const EdgeInsets.symmetric(horizontal: S.s12),
        height: 42,
        child: Row(
          children: [
            Text(
              label,
              style: typeScale.body.regular.style(color: palette.text.primary),
            ),
            const Spacer(),
            Toggle(
              tokens: ToggleTokens.current,
              value: value,
              onChanged: onChanged,
            ),
          ],
        ),
      ),
    );
  }
}

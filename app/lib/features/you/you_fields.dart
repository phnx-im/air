// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/toggle/toggle.dart';
import 'package:air/ds/components/toggle/toggle_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/util/debouncer.dart';
import 'package:flutter/material.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// Fill for a filled module in the profile sections: a field row, a device row.
///
/// The two hosts sit on different surfaces. The phone screen sits on
/// `base.primary`, the two-pane detail pane on `base.quinary`, which in dark is
/// the same shade as `base.secondary` and would swallow a base-tier module
/// whole. `elevated.secondary` lifts it one step there, and matches
/// `base.secondary` in light, so light and the phone are both unchanged.
Color youModuleFill(BuildContext context) {
  final palette = SemanticPalette.of(context);
  return context.breakpoint.isSmall
      ? palette.backgroundBase.secondary
      : palette.backgroundElevated.secondary;
}

/// The field vocabulary the profile sections are built from: a filled row, a
/// switch row, and the two label styles that annotate them.
class FieldContainer extends StatelessWidget {
  const FieldContainer({
    super.key,
    required this.child,
    this.height = 42,
    this.onTap,
  });

  final Widget child;
  final double? height;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return DefaultTextStyle(
      style: Theme.of(context).textTheme.bodyLarge!.copyWith(
        color: palette.text.primary,
        fontSize: typeScale.body.regular.fontSize,
      ),
      child: InkWell(
        onTap: onTap,
        child: Container(
          decoration: BoxDecoration(
            color: youModuleFill(context),
            borderRadius: BorderRadius.circular(CornerRadius.px16),
          ),
          padding: const EdgeInsets.symmetric(horizontal: S.s12),
          height: height,
          child: child,
        ),
      ),
    );
  }
}

class FieldLabel extends StatelessWidget {
  const FieldLabel(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: S.s8),
      child: Text(
        text,
        style: typeScale.body.xs.style(
          color: SemanticPalette.of(context).text.quaternary,
        ),
      ),
    );
  }
}

class SectionHeader extends StatelessWidget {
  const SectionHeader({super.key, required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: S.s8),
      child: Text(
        text,
        style: typeScale.body.regular.style(
          weight: Weight.emphasized,
          color: SemanticPalette.of(context).text.secondary,
        ),
      ),
    );
  }
}

/// A switch field that toggles a [ValueNotifier] optimistically and submits
/// the final value after a debounce delay.
class SwitchField extends HookWidget {
  const SwitchField({
    super.key,
    required this.onSubmit,
    required this.value,
    required this.label,
  });

  final Function(bool) onSubmit;
  final ValueNotifier<bool> value;
  final String label;

  @override
  Widget build(BuildContext context) {
    final debouncer = useMemoized(
      () => Debouncer(delay: const Duration(milliseconds: 500)),
    );
    // Flush on dispose: a tap still waiting out the debounce delay when the
    // screen closes is submitted, not dropped.
    useEffect(() => debouncer.flush, [debouncer]);

    // Only user taps schedule a submit. Programmatic writes to `value` (such
    // as an owner converging it onto cubit state) never do, so a state update
    // cannot re-trigger a submit and loop back on itself. Each tap captures
    // its intended value, and the debouncer retains only the latest action.
    final handleTap = useCallback(() {
      final submittedValue = !value.value;
      value.value = submittedValue;
      debouncer.run(() {
        onSubmit(submittedValue);
      });
    }, [onSubmit, value]);

    return FieldContainer(
      onTap: handleTap,
      child: Row(
        children: [
          Text(label, style: typeScale.body.regular.style()),
          const Spacer(),
          Toggle(
            tokens: ToggleTokens.current,
            value: value.value,
            onChanged: (_) => handleTap(),
          ),
        ],
      ),
    );
  }
}

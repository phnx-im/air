// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart' show showDialog;
import 'package:flutter/widgets.dart';

/// The pages of a modal that are holding input their user has not sent yet.
///
/// A page registers a question rather than an answer, and the answer is taken
/// at the moment someone leaves: a keystroke that fills a field changes nothing
/// on screen, so nothing here is worth a rebuild.
class ModalUnsavedInput {
  final _pages = <Object, ValueGetter<bool>>{};

  /// Whether any page of the modal is holding something right now.
  bool get any => _pages.values.any((holds) => holds());

  void _register(Object page, ValueGetter<bool> holds) => _pages[page] = holds;

  void _unregister(Object page) => _pages.remove(page);
}

/// Hands the modal's [ModalUnsavedInput] to the pages below it.
class ModalUnsavedInputScope extends InheritedWidget {
  const ModalUnsavedInputScope({
    super.key,
    required this.unsaved,
    required super.child,
  });

  final ModalUnsavedInput unsaved;

  static ModalUnsavedInput? maybeOf(BuildContext context) => context
      .dependOnInheritedWidgetOfExactType<ModalUnsavedInputScope>()
      ?.unsaved;

  @override
  bool updateShouldNotify(ModalUnsavedInputScope oldWidget) =>
      unsaved != oldWidget.unsaved;
}

/// Marks [child] as holding input its user would lose, so every way out of the
/// modal around it asks before dropping it.
///
/// Takes a getter rather than a flag, so a field filled between builds still
/// counts and filling one costs no rebuild.
class ModalDismissGuard extends StatefulWidget {
  const ModalDismissGuard({
    super.key,
    required this.hasUnsavedInput,
    required this.child,
  });

  final ValueGetter<bool> hasUnsavedInput;
  final Widget child;

  @override
  State<ModalDismissGuard> createState() => _ModalDismissGuardState();
}

class _ModalDismissGuardState extends State<ModalDismissGuard> {
  ModalUnsavedInput? _unsaved;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _unsaved?._unregister(this);
    _unsaved = ModalUnsavedInputScope.maybeOf(context);
    // Through the widget rather than the getter it carried when this ran, so a
    // rebuild that changes what dirty means needs nothing here.
    _unsaved?._register(this, () => widget.hasUnsavedInput());
  }

  @override
  void dispose() {
    _unsaved?._unregister(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

/// [onDismiss], asked about first where [unsaved] is holding something.
///
/// Only where the modal floats as a card. Full-screen it is left the way every
/// other screen is, through a system back the pattern never sees, so a question
/// on the header's back alone would guard one way out of two.
VoidCallback? guardedDismiss(
  BuildContext context,
  ModalUnsavedInput? unsaved,
  VoidCallback? onDismiss,
) {
  if (onDismiss == null || unsaved == null) return onDismiss;

  return () async {
    final asks = !ModalShellTokens.isFullBleed(context) && unsaved.any;
    if (asks && !await _confirmClose(context)) return;
    onDismiss();
  };
}

Future<bool> _confirmClose(BuildContext context) async {
  final loc = AppLocalizations.of(context);

  final close = await showDialog<bool>(
    context: context,
    builder: (_) => ConfirmDialog(
      title: loc.unsavedInputDialog_title,
      cancel: loc.unsavedInputDialog_cancel,
      confirm: loc.unsavedInputDialog_close,
      destructive: true,
    ),
  );

  return close ?? false;
}

// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';
import 'dart:math';

import 'package:air/ds/foundations/dimensions.dart';
import 'package:air/features/message_list/widgets/suggestion_overlay.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';

/// Gap between the caret and the overlay's left edge.
const _caretGap = S.s24;

/// Gap between the caret and the overlay's bottom edge.
const _caretRise = S.s12;

/// Represents a token in the text field that should be autocompleted.
class AutocompleteTrigger {
  const AutocompleteTrigger({
    required this.start,
    required this.end,
    required this.query,
  });

  /// Inclusive index where the trigger begins (e.g., the colon position).
  final int start;

  /// Exclusive index where the trigger ends (typically the caret position).
  final int end;

  /// The substring after the trigger character that the user typed.
  final String query;

  @override
  bool operator ==(Object other) {
    return other is AutocompleteTrigger &&
        start == other.start &&
        end == other.end &&
        query == other.query;
  }

  @override
  int get hashCode => Object.hash(start, end, query);
}

/// Describes how to detect triggers, fetch suggestions, and render them.
abstract class TextAutocompleteStrategy<T> {
  /// Inspect the current text value and return a trigger if one is active.
  AutocompleteTrigger? findTrigger(TextEditingValue value);

  /// Produce suggestions for the given query fragment.
  FutureOr<List<T>> suggestionsFor(String query);

  /// Apply a suggestion to the text, replacing the trigger range.
  TextEditingValue applySuggestion(
    TextEditingValue value,
    AutocompleteTrigger trigger,
    T suggestion,
  );

  /// Provide styling for the suggestion overlay.
  SuggestionOverlayStyle overlayStyle(BuildContext context);

  /// Build the per-row widget for each suggestion item.
  Widget buildSuggestionItem(
    BuildContext context,
    T suggestion,
    bool isHighlighted,
  );
}

/// Drives autocompletion for a text field using a pluggable strategy.
class TextAutocompleteController<T> {
  TextAutocompleteController({
    required this._textController,
    required FocusNode focusNode,
    required this._inputFieldKey,
    required LayerLink anchorLink,
    required TickerProvider vsync,
    required this._contextProvider,
    required this._strategy,
  }) : _focusNode = focusNode,
       _overlayController = SuggestionOverlayController<T>(
         vsync: vsync,
         anchorLink: anchorLink,
         focusNode: focusNode,
       ) {
    _overlayController.onSizeChanged = (_) {
      final offset = _calculateOverlayOffset();
      if (offset != null) {
        _overlayController.updateOffset(offset);
      }
    };
  }

  final TextEditingController _textController; // Source controller we watch.
  final FocusNode _focusNode; // Shared focus node for keyboard handling.
  final GlobalKey _inputFieldKey; // Key for locating the text field RenderBox.
  final BuildContext Function() _contextProvider; // Provides latest context.
  final TextAutocompleteStrategy<T> _strategy; // Domain-specific behavior.
  final SuggestionOverlayController<T> _overlayController; // Overlay driver.

  AutocompleteTrigger? _activeTrigger; // Trigger currently being autocompleted.
  int _requestToken = 0; // Incremented to discard stale async results.

  /// Dispose the overlay controller and any attached resources.
  void dispose() {
    _overlayController.dispose();
  }

  /// Sync overlay visibility with focus changes, respecting pointer taps.
  void handleFocusChange() {
    if (!_focusNode.hasFocus) {
      if (_overlayController.isPointerDown) {
        _focusNode.requestFocus();
        return;
      }
      unawaited(_overlayController.dismiss());
    } else {
      handleTextChanged();
    }
  }

  /// Route navigation keys (arrows, enter/tab, escape) to the overlay.
  KeyEventResult? handleKeyEvent(KeyEvent evt) {
    if (!_overlayController.isVisible) {
      return null;
    }
    final modifierKeyPressed =
        HardwareKeyboard.instance.isShiftPressed ||
        HardwareKeyboard.instance.isAltPressed ||
        HardwareKeyboard.instance.isMetaPressed ||
        HardwareKeyboard.instance.isControlPressed;

    if (evt is! KeyDownEvent && evt is! KeyRepeatEvent) {
      return null;
    }

    if (!modifierKeyPressed && evt.logicalKey == LogicalKeyboardKey.arrowDown) {
      _overlayController.moveHighlight(1);
      return KeyEventResult.handled;
    }
    if (!modifierKeyPressed && evt.logicalKey == LogicalKeyboardKey.arrowUp) {
      _overlayController.moveHighlight(-1);
      return KeyEventResult.handled;
    }
    if (!modifierKeyPressed && evt.logicalKey == LogicalKeyboardKey.pageDown) {
      _overlayController.movePage(1);
      return KeyEventResult.handled;
    }
    if (!modifierKeyPressed && evt.logicalKey == LogicalKeyboardKey.pageUp) {
      _overlayController.movePage(-1);
      return KeyEventResult.handled;
    }
    if (evt is KeyDownEvent &&
        !modifierKeyPressed &&
        (evt.logicalKey == LogicalKeyboardKey.enter ||
            evt.logicalKey == LogicalKeyboardKey.tab)) {
      // Enter/Tab confirm the highlighted suggestion.
      if (_overlayController.selectHighlighted()) {
        return KeyEventResult.handled;
      }
    }
    if (evt is KeyDownEvent && evt.logicalKey == LogicalKeyboardKey.escape) {
      // Escape closes the overlay but keeps focus in the field.
      unawaited(_overlayController.dismiss());
      return KeyEventResult.handled;
    }
    return null;
  }

  /// Re-scan the text when it changes and refresh the overlay suggestions.
  void handleTextChanged() {
    if (!_focusNode.hasFocus) {
      // Ignore updates when the field lost focus.
      unawaited(_overlayController.dismiss());
      return;
    }
    final valueSnapshot = _textController.value;
    final trigger = _strategy.findTrigger(valueSnapshot);
    _activeTrigger = trigger;
    if (trigger == null) {
      // No matching trigger, so hide the overlay.
      unawaited(_overlayController.dismiss());
      return;
    }
    _loadSuggestions(trigger);
  }

  /// Hide the overlay, returning immediately.
  void dismiss() {
    unawaited(_overlayController.dismiss());
  }

  /// Fetch strategy suggestions for the current trigger and show the overlay.
  Future<void> _loadSuggestions(AutocompleteTrigger trigger) async {
    // Increment the request token so stale async responses can be ignored.
    final requestId = ++_requestToken;
    final results = await Future<List<T>>.value(
      _strategy.suggestionsFor(trigger.query),
    );
    if (_activeTrigger != trigger || requestId != _requestToken) {
      // Trigger changed while awaiting results; drop this response.
      return;
    }
    if (results.isEmpty) {
      _activeTrigger = null;
      await _overlayController.dismiss();
      return;
    }
    final offset = _calculateOverlayOffset();
    if (offset == null) {
      // Can't position the overlay, so dismiss quietly.
      await _overlayController.dismiss();
      return;
    }
    final context = _contextProvider();
    if (!context.mounted) {
      await _overlayController.dismiss();
      return;
    }
    final style = _strategy.overlayStyle(context);
    await _overlayController.show(
      context: context,
      offset: offset,
      suggestions: results,
      style: style,
      itemBuilder: (ctx, item, isHighlighted) =>
          _strategy.buildSuggestionItem(ctx, item, isHighlighted),
      onSelected: _applySuggestion,
    );
  }

  /// Apply the approved suggestion to the text field and hide the overlay.
  void _applySuggestion(T suggestion) {
    final trigger = _activeTrigger;
    if (trigger == null) {
      unawaited(_overlayController.dismiss());
      return;
    }
    final newValue = _strategy.applySuggestion(
      _textController.value,
      trigger,
      suggestion,
    );
    _textController.value = newValue;
    _activeTrigger = null;
    unawaited(_overlayController.dismiss());
  }

  /// Compute the overlay offset relative to the caret position.
  Offset? _calculateOverlayOffset() {
    final fieldContext = _inputFieldKey.currentContext;
    final fieldBox = fieldContext?.findRenderObject() as RenderBox?;
    final renderEditable = _findRenderEditable();
    if (fieldBox == null || renderEditable == null) {
      // Without the field or editable, we cannot anchor the overlay.
      return null;
    }
    final selection = _textController.selection;
    if (!selection.isValid ||
        selection.extentOffset > _textController.text.length) {
      return null;
    }
    final caretRect = renderEditable.getLocalRectForCaret(selection.extent);
    final caretInField = renderEditable.localToGlobal(
      caretRect.topRight,
      ancestor: fieldBox,
    );
    final desired = caretInField + const Offset(_caretGap, -_caretRise);
    return _clampIntoView(desired, fieldBox, _overlayController.overlaySize);
  }

  /// Pull [desired] back so the overlay box stays inside the safe area.
  ///
  /// Takes and returns offsets in the input field's coordinate space, which is
  /// what the overlay's [CompositedTransformFollower] anchors against. Both
  /// position the overlay's bottom left corner, matching its follower anchor.
  Offset _clampIntoView(Offset desired, RenderBox fieldBox, Size overlaySize) {
    final context = _contextProvider();
    if (!context.mounted) {
      return desired;
    }
    final screen = MediaQuery.sizeOf(context);
    // viewPadding rather than padding, since an enclosing SafeArea may already
    // have consumed the latter while we reason in global coordinates.
    final inset = MediaQuery.viewPaddingOf(context);

    // Range the overlay's bottom left corner may occupy, in global
    // coordinates. Only the two clamped edges need the measured size, so a
    // stale one can no longer shift an overlay that already fits.
    final minX = inset.left + suggestionOverlayViewportMargin;
    final minY =
        inset.top + suggestionOverlayViewportMargin + overlaySize.height;
    final maxX =
        screen.width -
        inset.right -
        suggestionOverlayViewportMargin -
        overlaySize.width;
    final maxY = screen.height - inset.bottom - suggestionOverlayViewportMargin;

    // An overlay bigger than the safe area leaves no valid position, so favour
    // the top left corner instead of letting clamp assert on an empty range.
    final origin = fieldBox.localToGlobal(Offset.zero);
    final global = origin + desired;
    return Offset(
          global.dx.clamp(minX, max(minX, maxX)),
          global.dy.clamp(minY, max(minY, maxY)),
        ) -
        origin;
  }

  /// Find the RenderEditable that backs the focused text field.
  RenderEditable? _findRenderEditable() {
    final focusContext = _focusNode.context;
    if (focusContext == null) {
      return null;
    }
    final editableState = focusContext
        .findAncestorStateOfType<EditableTextState>();
    return editableState?.renderEditable;
  }
}

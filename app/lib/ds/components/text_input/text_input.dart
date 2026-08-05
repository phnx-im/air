// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/text_input/text_input_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// A filled field for one value, with an optional label above it and an
/// optional helper or error line below.
///
/// The host owns the text and the validation. Pass a [controller] to seed or
/// read the value, and set [errorText] to put the field in its error look: the
/// error line replaces the helper line and the field outlines itself in danger.
/// A host that renders the message itself raises [hasError] instead.
///
/// Carries the `App` prefix because `flutter/services.dart` already claims the
/// bare `TextInput`, so without it a host importing both would hit an
/// ambiguous reference.
class AppTextInput extends StatefulWidget {
  const AppTextInput({
    super.key,
    required this.tokens,
    this.controller,
    this.focusNode,
    this.label,
    this.hintText,
    this.helperText,
    this.errorText,
    this.hasError = false,
    this.onChanged,
    this.onSubmitted,
    this.enabled = true,
    this.autofocus = false,
    this.autocorrect = true,
    this.keyboardType,
    this.textInputAction,
    this.textCapitalization = TextCapitalization.none,
    this.inputFormatters,
    this.maxLength,
    this.minLines,
    this.maxLines = 1,
    this.style,
    this.textAlign = TextAlign.start,
    this.fieldPadding,
  });

  final AppTextInputTokens tokens;

  /// The host's controller. Without one the field keeps its own, which is
  /// enough when the host only follows it through [onChanged].
  final TextEditingController? controller;

  final FocusNode? focusNode;

  /// Muted line above the field, naming what it holds.
  final String? label;

  final String? hintText;

  /// Muted line below the field. Hidden while [errorText] is set, since they
  /// share the slot.
  final String? helperText;

  /// The failure to show below the field. Also switches the field's outline to
  /// danger.
  final String? errorText;

  /// Raises the error look on its own, for a host that owns the line below the
  /// field and would otherwise print its message twice. Implied by
  /// [errorText].
  final bool hasError;

  final ValueChanged<String>? onChanged;
  final ValueChanged<String>? onSubmitted;

  final bool enabled;
  final bool autofocus;
  final bool autocorrect;

  final TextInputType? keyboardType;
  final TextInputAction? textInputAction;

  /// Which shift state the software keyboard opens in. A field that only takes
  /// one case says so here, so the keyboard doesn't offer keys its formatters
  /// will drop.
  final TextCapitalization textCapitalization;

  final List<TextInputFormatter>? inputFormatters;

  /// Cap on the number of characters. Enforced silently: a host that wants a
  /// remaining-characters readout adds it as its own [helperText].
  final int? maxLength;

  /// Lines the field shows before it starts growing. Pairs with [maxLines] for
  /// a field that carries a paragraph.
  final int? minLines;

  /// Lines the field grows to. Null lets it grow without bound.
  final int maxLines;

  /// Merged over the field's body style, for a value the screen shows rather
  /// than collects, like a code read aloud and typed back.
  final TextStyle? style;

  /// Where the value sits in the field. Centred for a short code, so it reads
  /// as a display, not a sentence.
  final TextAlign textAlign;

  /// Replaces the tokens' inset around the text, for a field that has to stand
  /// taller than the density it sits at.
  final EdgeInsets? fieldPadding;

  @override
  State<AppTextInput> createState() => _AppTextInputState();
}

class _AppTextInputState extends State<AppTextInput> {
  TextEditingController? _fallbackController;

  TextEditingController get _controller =>
      widget.controller ?? (_fallbackController ??= TextEditingController());

  @override
  void dispose() {
    _fallbackController?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final tokens = widget.tokens;
    final palette = SemanticPalette.of(context);
    final error = widget.errorText;
    final below = error ?? widget.helperText;
    final errored = widget.hasError || error != null;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (widget.label != null) ...[
          Padding(
            padding: tokens.labelPadding,
            child: Text(
              widget.label!,
              style: typeScale.body.s.style(color: palette.text.quaternary),
            ),
          ),
          SizedBox(height: tokens.labelGap),
        ],
        Container(
          padding: widget.fieldPadding ?? tokens.fieldPadding,
          decoration: BoxDecoration(
            color: palette.fill.tertiary,
            borderRadius: BorderRadius.circular(tokens.radius),
            // The outline is always there, transparent until it carries an
            // error, so raising one doesn't shift the text it wraps.
            border: Border.all(
              color: errored ? palette.function.danger : Colors.transparent,
              width: tokens.borderWidth,
            ),
          ),
          child: _buildField(palette),
        ),
        if (below != null) ...[
          SizedBox(height: tokens.helperGap),
          Padding(
            padding: tokens.helperPadding,
            child: Text(
              below,
              style: typeScale.body.s.style(
                color: errored
                    ? palette.function.danger
                    : palette.text.quaternary,
              ),
            ),
          ),
        ],
      ],
    );
  }

  Widget _buildField(SemanticPalette palette) {
    // A single-line field takes a tight line box, so its height follows the
    // padding, and a locked strut, so it doesn't jump when the first character
    // replaces the hint. A field that grows keeps the typescale's leading,
    // which a paragraph needs to stay readable.
    final singleLine = widget.maxLines == 1 && widget.minLines == null;
    final style = typeScale.body.regular
        .style(color: palette.text.primary)
        .merge(widget.style);
    final inputStyle = singleLine ? style.copyWith(height: 1.0) : style;

    return TextField(
      controller: _controller,
      focusNode: widget.focusNode,
      enabled: widget.enabled,
      autofocus: widget.autofocus,
      autocorrect: widget.autocorrect,
      keyboardType: widget.keyboardType,
      textInputAction: widget.textInputAction,
      textCapitalization: widget.textCapitalization,
      inputFormatters: widget.inputFormatters,
      maxLength: widget.maxLength,
      minLines: widget.minLines,
      maxLines: widget.maxLines,
      onChanged: widget.onChanged,
      onSubmitted: widget.onSubmitted,
      textAlign: widget.textAlign,
      style: inputStyle,
      strutStyle: singleLine
          ? StrutStyle.fromTextStyle(inputStyle, forceStrutHeight: true)
          : null,
      // The chrome is the container's, so the field itself draws nothing. We
      // spell out the borders and fill rather than let the ambient input
      // theme add its own.
      decoration: InputDecoration(
        isDense: true,
        contentPadding: EdgeInsets.zero,
        filled: false,
        border: InputBorder.none,
        enabledBorder: InputBorder.none,
        disabledBorder: InputBorder.none,
        focusedBorder: InputBorder.none,
        errorBorder: InputBorder.none,
        focusedErrorBorder: InputBorder.none,
        counterText: '',
        hintText: widget.hintText,
        hintStyle: inputStyle.copyWith(color: palette.text.tertiary),
      ),
    );
  }
}

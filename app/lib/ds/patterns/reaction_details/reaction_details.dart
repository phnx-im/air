// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/avatar/avatar.dart';
import 'package:air/ds/components/emoji/centered_emoji.dart';
import 'package:air/ds/components/list_row/list_row.dart';
import 'package:air/ds/components/list_row/list_row_tokens.dart';
import 'package:air/ds/components/scroll/scroll_edges.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/reaction_details/reaction_details_tokens.dart';
import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/widgets.dart';

/// One reactor and the emoji they applied.
@immutable
class ReactionDetailEntry {
  const ReactionDetailEntry({
    required this.displayName,
    required this.emoji,
    this.image,
    this.gradientSeed,
    this.mine = false,
  });

  /// Shown as the row's label, so the host localizes the current user's own
  /// name ("You") before handing the entry over.
  final String displayName;

  final String emoji;

  /// Avatar picture, already decoded. Absent falls back to the initial.
  final ImageProvider? image;

  /// Seeds the avatar's fallback color. An account identifier keeps the circle
  /// stable where two reactors share a display name.
  final String? gradientSeed;

  /// True for the current user's own reactions, which get a remove action.
  final bool mine;
}

/// The "who reacted" viewer shown when tapping a reaction on a message.
///
/// A section selector across the top switches between every reaction and one
/// tab per distinct emoji. The current user's own rows carry a remove action,
/// the way to take a reaction back. Closing is the host sheet's business, so
/// the viewer has no close control of its own.
///
/// A pure view: reactors arrive resolved, in the order they should appear,
/// and every piece of copy is a parameter.
class ReactionDetails extends StatefulWidget {
  const ReactionDetails({
    super.key,
    required this.tokens,
    required this.entries,
    required this.allLabel,
    required this.removeLabel,
    this.initialEmoji,
    this.onRemove,
  });

  final ReactionDetailsTokens tokens;

  /// Every reactor, once per emoji they applied.
  final List<ReactionDetailEntry> entries;

  /// Label of the leading tab, the count included ("All 12").
  final String allLabel;

  /// Label of the action on the current user's own rows.
  final String removeLabel;

  /// Emoji whose tab opens selected. Null, or an emoji nobody reacted with,
  /// opens on the leading tab.
  final String? initialEmoji;

  /// Take back one of the current user's own reactions.
  final void Function(String emoji)? onRemove;

  @override
  State<ReactionDetails> createState() => _ReactionDetailsState();
}

class _ReactionDetailsState extends State<ReactionDetails> {
  /// Selected section. Null is the leading tab, otherwise a specific emoji.
  String? _emoji;

  final _tabScroll = ScrollController();

  /// How the tab strip rests against its ends, watched by the trailing fade.
  /// The strip scrolls horizontally, so "top" is its leading edge and "bottom"
  /// its trailing one. Starts at rest against both: a strip that fits never
  /// scrolls, and assuming otherwise flashes the fade on the first frame.
  final _tabEdges = ValueNotifier<ScrollEdges>(
    const ScrollEdges(atTop: true, atBottom: true),
  );

  @override
  void initState() {
    super.initState();
    _emoji = widget.initialEmoji;
    _tabScroll.addListener(_onTabScroll);
    WidgetsBinding.instance.addPostFrameCallback((_) => _onTabScroll());
  }

  @override
  void dispose() {
    _tabScroll.removeListener(_onTabScroll);
    _tabScroll.dispose();
    _tabEdges.dispose();
    super.dispose();
  }

  void _onTabScroll() {
    if (!mounted || !_tabScroll.hasClients) return;
    _tabEdges.value = ScrollEdges.of(_tabScroll.position);
  }

  @override
  Widget build(BuildContext context) {
    final tokens = widget.tokens;

    // Distinct emojis in first-seen order, with counts, for the tab strip.
    final order = <String>[];
    final counts = <String, int>{};
    for (final entry in widget.entries) {
      if (!counts.containsKey(entry.emoji)) order.add(entry.emoji);
      counts[entry.emoji] = (counts[entry.emoji] ?? 0) + 1;
    }

    final selected = order.contains(_emoji) ? _emoji : null;
    final visible = selected == null
        ? widget.entries
        : widget.entries.where((entry) => entry.emoji == selected).toList();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: tokens.tabStripPadding,
          child: _tabStrip(order, counts, selected),
        ),
        SizedBox(height: tokens.tabStripBottomGap),
        Expanded(child: _reactorList(context, visible)),
      ],
    );
  }

  Widget _tabStrip(
    List<String> order,
    Map<String, int> counts,
    String? selected,
  ) {
    final tokens = widget.tokens;
    return Stack(
      children: [
        // Metrics notifications cover what a scroll can't: the initial
        // layout, and tabs arriving or leaving while the strip sits still.
        NotificationListener<ScrollMetricsNotification>(
          onNotification: (notification) {
            _tabEdges.value = ScrollEdges.of(notification.metrics);
            return false;
          },
          child: SingleChildScrollView(
            controller: _tabScroll,
            scrollDirection: Axis.horizontal,
            child: Row(
              children: [
                _Tab(
                  tokens: tokens,
                  selected: selected == null,
                  onTap: () => setState(() => _emoji = null),
                  child: _TabText(
                    text: widget.allLabel,
                    selected: selected == null,
                  ),
                ),
                for (final emoji in order) ...[
                  SizedBox(width: tokens.tabGap),
                  _Tab(
                    tokens: tokens,
                    selected: selected == emoji,
                    onTap: () => setState(() => _emoji = emoji),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        CenteredEmoji(emoji: emoji, style: _glyphStyle),
                        SizedBox(width: tokens.tabCountGap),
                        _TabText(
                          text: '${counts[emoji] ?? 0}',
                          selected: selected == emoji,
                        ),
                      ],
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
        Positioned(
          top: 0,
          bottom: 0,
          right: 0,
          width: ReactionDetailsTokens.tabFadeWidth,
          child: _TrailingFade(edges: _tabEdges),
        ),
      ],
    );
  }

  Widget _reactorList(BuildContext context, List<ReactionDetailEntry> entries) {
    final rowTokens = ListRowTokens.of(context);
    return ListView.builder(
      padding: EdgeInsets.zero,
      itemCount: entries.length,
      itemBuilder: (context, index) => _ReactorRow(
        tokens: widget.tokens,
        rowTokens: rowTokens,
        entry: entries[index],
        removeLabel: widget.removeLabel,
        separator: index < entries.length - 1,
        onRemove: widget.onRemove,
      ),
    );
  }
}

/// Emoji glyphs sit alongside the row label rather than standing on their own,
/// so they take the largest body step instead of the emoji scale.
TextStyle get _glyphStyle => typeScale.body.l.style();

/// One section of the selector: a pill that fills once selected.
class _Tab extends StatelessWidget {
  const _Tab({
    required this.tokens,
    required this.selected,
    required this.onTap,
    required this.child,
  });

  final ReactionDetailsTokens tokens;
  final bool selected;
  final VoidCallback onTap;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: Container(
          padding: tokens.tabPadding,
          decoration: BoxDecoration(
            color: selected ? palette.fill.tertiary : null,
            borderRadius: BorderRadius.circular(
              ReactionDetailsTokens.tabRadius,
            ),
          ),
          child: child,
        ),
      ),
    );
  }
}

class _TabText extends StatelessWidget {
  const _TabText({required this.text, required this.selected});

  final String text;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return Text(
      text,
      style: typeScale.body.s
          .style(
            color: selected ? palette.text.primary : palette.text.secondary,
          )
          .copyWith(height: 1.0),
    );
  }
}

/// Fade over the trailing edge of the tab strip, signalling tabs off-screen.
/// It masks against the elevated surface every host sheet paints, so a tab
/// dissolves into the sheet rather than getting clipped at its edge.
class _TrailingFade extends StatelessWidget {
  const _TrailingFade({required this.edges});

  final ValueListenable<ScrollEdges> edges;

  @override
  Widget build(BuildContext context) {
    final color = SemanticPalette.of(context).backgroundElevated.primary;
    return IgnorePointer(
      child: ValueListenableBuilder<ScrollEdges>(
        valueListenable: edges,
        child: DecoratedBox(
          decoration: BoxDecoration(
            gradient: LinearGradient(
              begin: Alignment.centerLeft,
              end: Alignment.centerRight,
              colors: [
                color.withValues(alpha: Alpha.a0),
                color,
              ],
            ),
          ),
        ),
        builder: (context, value, child) => AnimatedOpacity(
          opacity: value.atBottom ? 0 : 1,
          duration: Effect.duration(MotionPreset.short),
          curve: Effect.easeOutQuart,
          child: child,
        ),
      ),
    );
  }
}

class _ReactorRow extends StatelessWidget {
  const _ReactorRow({
    required this.tokens,
    required this.rowTokens,
    required this.entry,
    required this.removeLabel,
    required this.separator,
    required this.onRemove,
  });

  final ReactionDetailsTokens tokens;
  final ListRowTokens rowTokens;
  final ReactionDetailEntry entry;
  final String removeLabel;
  final bool separator;
  final void Function(String emoji)? onRemove;

  @override
  Widget build(BuildContext context) {
    final glyph = CenteredEmoji(emoji: entry.emoji, style: _glyphStyle);
    final onRemove = entry.mine ? this.onRemove : null;

    return ListRow(
      tokens: rowTokens,
      label: entry.displayName,
      separator: separator,
      leading: Avatar(
        displayName: entry.displayName,
        size: tokens.avatarSize,
        image: entry.image,
        gradientSeed: entry.gradientSeed,
      ),
      trailing: onRemove == null
          ? glyph
          : Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                _RemoveAction(
                  label: removeLabel,
                  onTap: () => onRemove(entry.emoji),
                ),
                SizedBox(width: tokens.removeGap),
                glyph,
              ],
            ),
    );
  }
}

class _RemoveAction extends StatelessWidget {
  const _RemoveAction({required this.label, required this.onTap});

  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: Text(
          label,
          style: typeScale.body.mini
              .style(color: palette.function.danger)
              .copyWith(height: 1.0),
        ),
      ),
    );
  }
}

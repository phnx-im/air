// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/core/core.dart' hide LogEntry;
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/components/scaffold/app_scaffold.dart';
import 'package:air/ds/components/scroll/app_scrollbar.dart';
import 'package:air/ds/components/searchfield/searchfield.dart';
import 'package:air/ds/components/searchfield/searchfield_tokens.dart';
import 'package:air/ds/components/state_layer/state_layer.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu.dart';
import 'package:air/features/developer/developer_fields.dart';
import 'package:air/features/developer/log_entry.dart';
import 'package:air/platform/method_channel.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart'
    show CircularProgressIndicator, MaterialPageRoute, SelectableText, SnackBar;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_hooks/flutter_hooks.dart';
import 'package:intl/intl.dart';
import 'package:share_plus/share_plus.dart';

/// Pushes the log viewer.
///
/// Pageless, like the chat debug view, so a developer screen stays out of the
/// navigation state.
void showLogs(BuildContext context) {
  Navigator.of(
    context,
  ).push(MaterialPageRoute(builder: (_) => const LogsScreen()));
}

/// Which ring buffer is being read. The app and the background isolate each
/// write their own.
enum LogBuffer {
  app('App'),
  background('Background');

  const LogBuffer(this.label);

  final String label;
}

/// Reads a log buffer and turns it into records for [LogsScreenView].
class LogsScreen extends HookWidget {
  const LogsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final buffer = useState(LogBuffer.app);
    final reloadKey = useState(0);
    final filter = useState(const LogFilter());
    final following = useState(false);

    // Following means polling: readAppLogs hands over the whole buffer, there
    // is nothing to subscribe to. The tick is cheap, an unchanged buffer stops
    // at the memo below.
    useEffect(() {
      if (!following.value) return null;
      final timer = Timer.periodic(
        const Duration(seconds: 1),
        (_) => reloadKey.value++,
      );
      return timer.cancel;
    }, [following.value]);

    final raw = useFuture(
      useMemoized(() => _readBuffer(buffer.value), [
        buffer.value,
        reloadKey.value,
      ]),
    );

    final loaded = raw.data;
    final current = loaded?.buffer == buffer.value ? loaded : null;

    // Keyed on the text, so a poll without new bytes reuses the records.
    final text = current?.text ?? '';
    final entries = useMemoized(() => parseLogs(text), [text]);

    return LogsScreenView(
      buffer: buffer.value,
      onBufferChanged: (value) => buffer.value = value,
      filter: filter.value,
      onFilterChanged: (value) => filter.value = value,
      following: following.value,
      onFollowingChanged: (value) => following.value = value,
      entries: entries,
      loading:
          raw.connectionState == ConnectionState.waiting && current == null,
      error: current?.error,
      onReload: () => reloadKey.value++,
      onClear: () async {
        await _clearBuffers();
        reloadKey.value++;
      },
    );
  }
}

/// One read of one buffer, tagged with the buffer it came from.
///
/// The snapshot keeps the previous read while the next is in flight, so a poll
/// never blanks the list. The tag tells a preserved read of the other buffer
/// apart from one of the buffer now selected, failed reads included.
typedef _BufferRead = ({LogBuffer buffer, String text, Object? error});

Future<_BufferRead> _readBuffer(LogBuffer buffer) async {
  try {
    return (
      buffer: buffer,
      text: switch (buffer) {
        LogBuffer.app => await readAppLogs(),
        LogBuffer.background => await readBackgroundLogs(
          cacheDir: await getCacheDirectory(),
        ),
      },
      error: null,
    );
  } catch (error) {
    return (buffer: buffer, text: '', error: error);
  }
}

/// Clears both buffers, since a run worth clearing spans them.
Future<void> _clearBuffers() async {
  await clearAppLogs();
  await clearBackgroundLogs(cacheDir: await getCacheDirectory());
}

/// The log buffer as a list of records, newest first, narrowed by [filter].
///
/// We filter here rather than in the host, so the list and the note standing
/// in for it when nothing matches read the same inputs.
class LogsScreenView extends StatelessWidget {
  const LogsScreenView({
    required this.buffer,
    required this.onBufferChanged,
    required this.filter,
    required this.onFilterChanged,
    required this.following,
    required this.onFollowingChanged,
    required this.entries,
    required this.loading,
    required this.error,
    required this.onReload,
    required this.onClear,
    super.key,
  });

  final LogBuffer buffer;
  final ValueChanged<LogBuffer> onBufferChanged;

  final LogFilter filter;
  final ValueChanged<LogFilter> onFilterChanged;

  /// Whether the list stays pinned to the newest record as more arrive.
  final bool following;
  final ValueChanged<bool> onFollowingChanged;

  final List<LogEntry> entries;
  final bool loading;
  final Object? error;

  final VoidCallback onReload;
  final VoidCallback onClear;

  @override
  Widget build(BuildContext context) {
    final visible = entries.where(filter.matches).toList();

    return AppScaffold(
      title: 'Logs',
      scrollable: false,
      // Pushed over the whole window, so the back button sits in the window's
      // own corner.
      reserveWindowControls: Chrome.windowControlsFloat,
      trailing: _LogsMenuButton(onReload: onReload, onClear: onClear),
      child: Column(
        crossAxisAlignment: .stretch,
        children: [
          _FilterBar(
            buffer: buffer,
            onBufferChanged: onBufferChanged,
            filter: filter,
            onFilterChanged: onFilterChanged,
            following: following,
            onFollowingChanged: onFollowingChanged,
          ),
          Expanded(child: _body(context, visible)),
        ],
      ),
    );
  }

  Widget _body(BuildContext context, List<LogEntry> visible) {
    final palette = SemanticPalette.of(context);

    if (error case final error?) {
      return Center(
        child: Text(
          error.toString(),
          style: typeScale.body.s.style(color: palette.text.secondary),
        ),
      );
    }
    if (visible.isEmpty) {
      return Center(
        child: loading
            ? SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(
                  strokeWidth: StrokeWidth.px2,
                  valueColor: AlwaysStoppedAnimation<Color>(
                    palette.text.primary,
                  ),
                ),
              )
            : Text(
                entries.isEmpty ? 'No records' : 'No records match',
                style: typeScale.body.s.style(color: palette.text.tertiary),
              ),
      );
    }

    return _LogList(
      days: groupByDay(visible),
      following: following,
      onScrolledAway: () => onFollowingChanged(false),
      onTargetTap: (target) => onFilterChanged(filter.copyWith(target: target)),
    );
  }
}

/// The records under the day each of them falls on, built lazily so the
/// buffer's size does not drive what a frame costs.
///
/// Each day is a group with a pinned header, so the header is both the break
/// between two days and the answer to "which day is this" inside one.
class _LogList extends HookWidget {
  const _LogList({
    required this.days,
    required this.following,
    required this.onScrolledAway,
    required this.onTargetTap,
  });

  final List<LogDay> days;
  final bool following;

  /// Called when a drag moves the list, so following stops instead of pulling
  /// the reader back to the top.
  final VoidCallback onScrolledAway;

  final ValueChanged<String> onTargetTap;

  @override
  Widget build(BuildContext context) {
    final controller = useScrollController();

    // The newest record is the first, so following holds offset zero. We hold
    // it after the frame: a jump dispatches scroll notifications
    // synchronously, and the app bar listens for those.
    useEffect(() {
      if (!following) return null;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (controller.hasClients) controller.jumpTo(0);
      });
      return null;
    }, [days, following]);

    return AppScrollbar(
      child: NotificationListener<ScrollStartNotification>(
        onNotification: (notification) {
          if (following && notification.dragDetails != null) {
            onScrolledAway();
          }
          return false;
        },
        child: ScrollConfiguration(
          behavior: ScrollConfiguration.of(context).copyWith(scrollbars: false),
          child: CustomScrollView(
            controller: controller,
            slivers: [
              for (final day in days)
                SliverMainAxisGroup(
                  slivers: [
                    SliverPersistentHeader(
                      pinned: true,
                      delegate: _DayHeaderDelegate(day.date),
                    ),
                    SliverList.builder(
                      itemCount: day.entries.length,
                      itemBuilder: (context, index) => LogRow(
                        entry: day.entries[index],
                        onTargetTap: () =>
                            onTargetTap(day.entries[index].target),
                      ),
                    ),
                  ],
                ),
              SliverToBoxAdapter(
                child: SizedBox(
                  height: MediaQuery.viewPaddingOf(context).bottom + S.s16,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// The day a run of records falls on, pinned while they scroll past and pushed
/// off by the next day's header.
class _DayHeaderDelegate extends SliverPersistentHeaderDelegate {
  const _DayHeaderDelegate(this.date);

  final DateTime date;

  @override
  double get minExtent => S.s28;

  @override
  double get maxExtent => S.s28;

  @override
  Widget build(
    BuildContext context,
    double shrinkOffset,
    bool overlapsContent,
  ) {
    final palette = SemanticPalette.of(context);

    return DecoratedBox(
      // Opaque, with the hairline on the edge the records pass under: the
      // header is pinned over them rather than scrolling with them.
      decoration: BoxDecoration(
        color: palette.backgroundBase.primary,
        border: Border(
          bottom: BorderSide(
            color: palette.separator.secondary,
            width: StrokeWidth.px0_5,
          ),
        ),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: S.s8),
        child: Align(
          alignment: Alignment.centerLeft,
          child: Text(
            // Written as the records are, not as Today or a weekday: this is
            // the date a server log is matched against.
            _dateFormat.format(date),
            style: typeScale.body.xs
                .style(weight: Weight.emphasized, color: palette.text.secondary)
                .withSystemMonospace(),
          ),
        ),
      ),
    );
  }

  @override
  bool shouldRebuild(_DayHeaderDelegate oldDelegate) =>
      oldDelegate.date != date;
}

/// What narrows the list: the buffer it reads, a query, a level threshold, and
/// the one target a row was tapped to isolate.
class _FilterBar extends StatelessWidget {
  const _FilterBar({
    required this.buffer,
    required this.onBufferChanged,
    required this.filter,
    required this.onFilterChanged,
    required this.following,
    required this.onFollowingChanged,
  });

  final LogBuffer buffer;
  final ValueChanged<LogBuffer> onBufferChanged;
  final LogFilter filter;
  final ValueChanged<LogFilter> onFilterChanged;
  final bool following;
  final ValueChanged<bool> onFollowingChanged;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return Column(
      crossAxisAlignment: .stretch,
      spacing: S.s8,
      children: [
        SearchField(
          tokens: SearchFieldTokens.current,
          hintText: 'Search',
          onChanged: (value) =>
              onFilterChanged(filter.copyWith(query: value.trim())),
        ),
        // Wrapped rather than scrolled sideways: seven pills do not fit a
        // phone, and a filter scrolled out of view is one nobody finds. The
        // wider gap between groups stands in for the dividers.
        Padding(
          padding: const EdgeInsets.only(bottom: S.s12),
          child: Wrap(
            spacing: S.s16,
            runSpacing: S.s8,
            crossAxisAlignment: .center,
            children: [
              // First, and so always on the top line: the one pill that is a
              // mode of the list rather than a filter over it.
              FilterPill(
                label: 'Follow',
                selected: following,
                onTap: () => onFollowingChanged(!following),
              ),
              if (filter.target case final target?)
                FilterPill(
                  label: target,
                  selected: true,
                  trailing: AppIcon.x(
                    size: S.s12,
                    color: palette.backgroundBase.primary,
                  ),
                  onTap: () =>
                      onFilterChanged(filter.copyWith(clearTarget: true)),
                ),
              _PillGroup(
                children: [
                  for (final value in LogBuffer.values)
                    FilterPill(
                      label: value.label,
                      selected: value == buffer,
                      onTap: () => onBufferChanged(value),
                    ),
                ],
              ),
              _PillGroup(
                children: [
                  for (final level in LogLevel.values)
                    FilterPill(
                      label: level.label,
                      selected: level == filter.threshold,
                      onTap: () =>
                          onFilterChanged(filter.copyWith(threshold: level)),
                    ),
                ],
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/// One record: a header line naming when, how loud and from where, with the
/// message under it.
class LogRow extends StatelessWidget {
  const LogRow({required this.entry, required this.onTargetTap, super.key});

  final LogEntry entry;

  /// Isolates the target. Its own gesture inside the row, so the common tap
  /// still opens the record.
  final VoidCallback onTargetTap;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    final mono = typeScale.body.xs
        .style(color: palette.text.tertiary)
        .withSystemMonospace();

    return StateLayer(
      borderRadius: CornerRadius.px8,
      surface: SemanticPalette.of(context).backgroundBase.primary,
      onTap: () => Navigator.of(
        context,
      ).push(MaterialPageRoute(builder: (_) => LogDetailScreen(entry: entry))),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: S.s8, vertical: S.s4),
        child: Column(
          crossAxisAlignment: .start,
          children: [
            Row(
              children: [
                Text(_timeFormat.format(entry.time.toLocal()), style: mono),
                const SizedBox(width: S.s8),
                Text(
                  entry.level.label.padRight(5),
                  style: mono.copyWith(
                    color: levelColor(palette, entry.level),
                    fontWeight: .w600,
                  ),
                ),
                const SizedBox(width: S.s8),
                Expanded(
                  child: GestureDetector(
                    behavior: .opaque,
                    onTap: onTargetTap,
                    child: Text(
                      entry.target,
                      maxLines: 1,
                      overflow: .ellipsis,
                      // The palette's only non-neutral accent, which is what
                      // sets the target apart from the message under it.
                      style: mono.copyWith(color: palette.function.link),
                    ),
                  ),
                ),
                if (entry.detail != null)
                  AppIcon.chevronRight(
                    size: S.s12,
                    color: palette.text.quaternary,
                  ),
              ],
            ),
            Text(
              entry.message,
              maxLines: 3,
              overflow: .ellipsis,
              style: typeScale.body.s
                  .style(color: palette.text.primary)
                  .withSystemMonospace(),
            ),
          ],
        ),
      ),
    );
  }
}

/// One record in full, with each part on a surface that copies on its own.
class LogDetailScreen extends StatelessWidget {
  const LogDetailScreen({required this.entry, super.key});

  final LogEntry entry;

  @override
  Widget build(BuildContext context) {
    return AppScaffold(
      title: entry.target.isEmpty ? 'Record' : entry.target,
      reserveWindowControls: Chrome.windowControlsFloat,
      child: Column(
        crossAxisAlignment: .stretch,
        spacing: S.s16,
        children: [
          _TextCard(caption: 'Message', text: entry.message),
          if (entry.detail case final detail?)
            _TextCard(caption: 'Detail', text: detail),
          DeveloperCard(
            caption: 'From',
            children: [
              DeveloperInfoRow(
                label: 'Target',
                value: entry.target,
                monospace: true,
              ),
              DeveloperInfoRow(label: 'Source', value: entry.source.name),
              DeveloperInfoRow(
                label: 'Level',
                value: entry.level.label,
                content: Text(
                  entry.level.label,
                  style: typeScale.body.s
                      .style(
                        color: levelColor(
                          SemanticPalette.of(context),
                          entry.level,
                        ),
                      )
                      .withSystemMonospace(),
                ),
              ),
              DeveloperInfoRow(
                label: 'Time',
                value: _dateTimeFormat.format(entry.time.toLocal()),
                monospace: true,
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// A selectable block of text, under its own caption and beside the button
/// that copies all of it.
class _TextCard extends StatelessWidget {
  const _TextCard({required this.caption, required this.text});

  final String caption;
  final String text;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);

    return Column(
      crossAxisAlignment: .stretch,
      children: [
        Row(
          children: [
            Expanded(child: DeveloperCaption(caption)),
            ButtonIcon(
              variant: ButtonIconVariant.plain,
              size: ButtonIconSize.s32,
              hitTargetSize: S.s40,
              icon: AppIconType.copy,
              onPressed: () {
                Clipboard.setData(ClipboardData(text: text));
                showSnackBarStandalone(
                  (loc) => SnackBar(
                    content: Text('Copied $caption'),
                    duration: const Duration(seconds: 2),
                  ),
                );
              },
            ),
          ],
        ),
        Container(
          padding: const EdgeInsets.all(S.s12),
          decoration: BoxDecoration(
            color: developerCardFill(context),
            borderRadius: BorderRadius.circular(CornerRadius.px12),
          ),
          child: SelectableText(
            text,
            style: typeScale.body.s
                .style(color: palette.text.primary)
                .withSystemMonospace(),
          ),
        ),
      ],
    );
  }
}

/// One run of pills: source, or level. Its own wrap, so the run stays on one
/// line wherever the width allows.
class _PillGroup extends StatelessWidget {
  const _PillGroup({required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) =>
      Wrap(spacing: S.s8, runSpacing: S.s8, children: children);
}

/// A pill that is either on or off, for the filters above a log list.
class FilterPill extends StatelessWidget {
  const FilterPill({
    required this.label,
    required this.selected,
    required this.onTap,
    this.trailing,
    super.key,
  });

  final String label;
  final bool selected;
  final VoidCallback onTap;

  /// Follows the label, for a pill whose tap clears the filter.
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final palette = SemanticPalette.of(context);
    // The brand accent is a neutral here, so we invert rather than tint: at
    // these sizes a tint does not read as on.
    final fill = selected ? palette.accentBrand.primary : palette.fill.tertiary;

    return StateLayer(
      borderRadius: CornerRadius.px8,
      surface: fill,
      onTap: onTap,
      background: DecoratedBox(
        decoration: BoxDecoration(
          color: fill,
          borderRadius: BorderRadius.circular(CornerRadius.px8),
        ),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: S.s12, vertical: S.s4),
        child: Row(
          mainAxisSize: .min,
          spacing: S.s4,
          children: [
            Text(
              label,
              style: typeScale.body.s.style(
                // One weight in both states: emphasizing the selected label
                // widens the pill, shifting the run under the finger.
                color: selected
                    ? palette.backgroundBase.primary
                    : palette.text.tertiary,
              ),
            ),
            ?trailing,
          ],
        ),
      ),
    );
  }
}

/// Save, share, reload and clear: the actions on the whole buffer.
class _LogsMenuButton extends StatelessWidget {
  const _LogsMenuButton({required this.onReload, required this.onClear});

  final VoidCallback onReload;
  final VoidCallback onClear;

  @override
  Widget build(BuildContext context) {
    return ButtonIcon(
      variant: ButtonIconVariant.plain,
      size: ButtonIconSize.s32,
      hitTargetSize: S.s40,
      icon: AppIconType.ellipsis,
      onPressed: () => _open(context),
    );
  }

  void _open(BuildContext context) {
    final render = context.findRenderObject();
    if (render is! RenderBox || !render.hasSize) return;

    unawaited(
      showOverlayMenu(
        context: context,
        anchor: render.localToGlobal(Offset.zero) & render.size,
        corner: MenuCorner.topRight,
        items: [
          if (DeviceType.isDesktop)
            MenuItem(
              label: 'Save',
              icon: AppIconType.download,
              onPressed: _saveLogs,
            ),
          if (DeviceType.isPhone)
            MenuItem(
              label: 'Share',
              icon: AppIconType.share,
              onPressed: _shareLogs,
            ),
          MenuItem(
            label: 'Reload',
            icon: AppIconType.refreshCw,
            onPressed: onReload,
          ),
          MenuItem(
            label: 'Clear',
            icon: AppIconType.trash,
            destructive: true,
            onPressed: onClear,
          ),
        ],
      ),
    );
  }

  void _shareLogs() async {
    final data = await tarLogs(cacheDir: await getCacheDirectory());
    final file = XFile.fromData(data, mimeType: 'application/gzip');
    SharePlus.instance.share(
      ShareParams(files: [file], fileNameOverrides: ['logs.tar.gz']),
    );
  }

  void _saveLogs() async {
    final data = await tarLogs(cacheDir: await getCacheDirectory());
    final result = await getSaveLocation(suggestedName: 'logs.tar.gz');
    if (result == null) {
      return;
    }
    await XFile.fromData(
      data,
      mimeType: 'application/gzip',
    ).saveTo(result.path);
  }
}

/// How loud a record is, in the colors a terminal would use for it.
Color levelColor(SemanticPalette palette, LogLevel level) => switch (level) {
  LogLevel.trace => palette.text.quaternary,
  LogLevel.debug => palette.text.tertiary,
  LogLevel.info => palette.function.success.primary,
  LogLevel.warn => palette.function.warning.primary,
  LogLevel.error => palette.function.danger,
};

extension LogLevelLabel on LogLevel {
  String get label => name.toUpperCase();
}

/// All three read local time: the buffer stores UTC, but records are matched
/// against what just happened on the device.
///
/// A row carries the clock alone, its day header carries the day. The detail
/// screen stands on its own and carries both.
final _timeFormat = DateFormat('HH:mm:ss.SSS');
final _dateFormat = DateFormat('yyyy-MM-dd');
final _dateTimeFormat = DateFormat('yyyy-MM-dd HH:mm:ss.SSS');

// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/platform/method_channel.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:air/core/core.dart';
import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/components/scroll/app_scrollbar.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_route.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu.dart';
import 'package:share_plus/share_plus.dart';

/// Opens the logs on the surface the device calls for.
Future<void> showLogs(BuildContext context) =>
    showAppModal<void>(context: context, builder: (_) => const LogsModal());

class LogsModal extends StatefulWidget {
  const LogsModal({super.key});

  @override
  State<LogsModal> createState() => _LogsModalState();
}

class _LogsModalState extends State<LogsModal> {
  late Future<String> _appLogs;
  late Future<String> _backgroundLogs;
  late Timer _timer;

  @override
  void initState() {
    super.initState();
    _loadLogs();
    _timer = Timer.periodic(const Duration(seconds: 1), (timer) {
      _loadLogs();
    });
  }

  @override
  void dispose() {
    _timer.cancel();
    super.dispose();
  }

  void _loadLogs() async {
    final appLogs = readAppLogs();
    final backgroundLogs = getCacheDirectory().then(
      (cacheDir) => readBackgroundLogs(cacheDir: cacheDir),
    );

    setState(() {
      _appLogs = appLogs;
      _backgroundLogs = backgroundLogs;
    });
  }

  void _clearLogs() async {
    await clearAppLogs();
    final cacheDir = await getCacheDirectory();
    await clearBackgroundLogs(cacheDir: cacheDir);
    setState(() {
      _appLogs = Future.value("");
      _backgroundLogs = Future.value("");
    });
  }

  @override
  Widget build(BuildContext context) {
    return LogsView(
      appLogs: _appLogs,
      backgroundLogs: _backgroundLogs,
      reloadLogs: _loadLogs,
      clearLogs: _clearLogs,
    );
  }
}

class LogsView extends StatelessWidget {
  const LogsView({
    required this.appLogs,
    required this.backgroundLogs,
    required this.reloadLogs,
    required this.clearLogs,
    super.key,
  });

  final Future<String> appLogs;
  final Future<String> backgroundLogs;
  final VoidCallback reloadLogs;
  final VoidCallback clearLogs;

  @override
  Widget build(BuildContext context) {
    return ModalScaffold(
      title: 'Logs',
      onDismiss: () => Navigator.of(context).pop(),
      trailing: _LogsMenuButton(
        items: (context) => [
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
            onPressed: reloadLogs,
          ),
          MenuItem(
            label: 'Clear',
            icon: AppIconType.trash,
            destructive: true,
            onPressed: clearLogs,
          ),
        ],
      ),
      // Each tab scrolls its own log text.
      scrollable: false,
      child: DefaultTabController(
        length: 2,
        child: Column(
          children: [
            const TabBar(
              tabs: [
                Tab(text: 'App'),
                Tab(text: 'Background'),
              ],
            ),
            Expanded(
              child: TabBarView(
                children: [
                  _LogsView(logs: appLogs),
                  _LogsView(logs: backgroundLogs),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _shareLogs() async {
    final cacheDir = await getCacheDirectory();
    final data = await tarLogs(cacheDir: cacheDir);
    final file = XFile.fromData(data, mimeType: 'application/gzip');
    SharePlus.instance.share(
      ShareParams(files: [file], fileNameOverrides: ['logs.tar.gz']),
    );
  }

  void _saveLogs() async {
    final cacheDir = await getCacheDirectory();
    final data = await tarLogs(cacheDir: cacheDir);

    const String fileName = 'logs.tar.gz';
    final FileSaveLocation? result = await getSaveLocation(
      suggestedName: fileName,
    );
    if (result == null) {
      // Operation was canceled by the user.
      return;
    }

    await XFile.fromData(
      data,
      mimeType: 'application/gzip',
    ).saveTo(result.path);
  }
}

class _LogsMenuButton extends StatelessWidget {
  const _LogsMenuButton({required this.items});

  final List<MenuItem> Function(BuildContext context) items;

  @override
  Widget build(BuildContext context) {
    return DialogHeaderAction(
      tokens: DialogHeaderTokens.of(context),
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
        items: items(context),
      ),
    );
  }
}

class _LogsView extends StatefulWidget {
  const _LogsView({required this.logs});

  final Future<String>? logs;

  @override
  State<_LogsView> createState() => _LogsViewState();
}

class _LogsViewState extends State<_LogsView>
    with AutomaticKeepAliveClientMixin {
  @override
  Widget build(BuildContext context) {
    super.build(context);
    return FutureBuilder(
      future: widget.logs,
      builder: (context, snapshot) {
        if (snapshot.hasData) {
          final data = snapshot.data!;
          // The text scrolls inside the tab's bounded height, with padding
          // clearing the home indicator since the surface runs full height.
          return AppScrollbar(
            child: SingleChildScrollView(
              padding: EdgeInsets.fromLTRB(
                ModalShellTokens.contentPaddingLeft,
                0,
                ModalShellTokens.contentPaddingRight,
                MediaQuery.viewPaddingOf(context).bottom,
              ),
              child: SelectableText(data),
            ),
          );
        } else if (snapshot.hasError) {
          return const Center(child: Text('Error loading logs'));
        }
        return const Center(
          child: SizedBox(child: CircularProgressIndicator()),
        );
      },
    );
  }

  @override
  bool get wantKeepAlive => true;
}

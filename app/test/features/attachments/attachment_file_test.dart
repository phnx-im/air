// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later
import 'dart:async';

import 'package:air/features/attachments/attachment_file.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_bubble/message_bubble.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';

const physicalSize = Size(800, 2150);

final file = UiAttachment(
  attachmentId: 42.attachmentId(),
  filename: 'example.bin',
  contentType: 'application/octet-stream',
  size: 10 * 1024 * 1024,
);

List<(Color, Color)> testColors(BuildContext context) {
  final palette = SemanticPalette.of(context);
  return [
    (palette.message.selfText, MessageBubble.fillOf(context, isSelf: true)),
    (palette.message.otherText, MessageBubble.fillOf(context, isSelf: false)),
  ];
}

Map<AttachmentId, UiAttachmentStatus> testStatuses = {
  1.attachmentId(): const UiAttachmentStatus.pending(),
  2.attachmentId(): UiAttachmentStatus.progress(BigInt.from(7 * 1024 * 1024)),
  3.attachmentId(): const UiAttachmentStatus.failed(),
  4.attachmentId(): const UiAttachmentStatus.notFound(),
  5.attachmentId(): const UiAttachmentStatus.completed(),
};

class _FileTestBubble extends StatelessWidget {
  const _FileTestBubble({
    required this.attachmentId,
    required this.color,
    required this.backgroundColor,
    this.attachment,
  });

  final AttachmentId attachmentId;
  final Color color;
  final Color backgroundColor;
  final UiAttachment? attachment;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: S.s16, vertical: S.s8),
      decoration: BoxDecoration(
        color: backgroundColor,
        borderRadius: BorderRadius.circular(CornerRadius.px20),
      ),
      child: AttachmentFile(
        attachment: attachment ?? file.copyWith(attachmentId: attachmentId),
        isSender: true,
        color: color,
      ),
    );
  }
}

void main() {
  setUpAll(() {
    registerFallbackValue(0.attachmentId());
  });

  late MockAttachmentsRepository attachmentsRepository;

  setUp(() async {
    attachmentsRepository = MockAttachmentsRepository();
    when(
      () => attachmentsRepository.statusStream(
        attachmentId: any(named: 'attachmentId'),
      ),
    ).thenAnswer(
      (invocation) => Stream.value(
        testStatuses[invocation.namedArguments[#attachmentId]
                as AttachmentId] ??
            const UiAttachmentStatus.completed(),
      ),
    );
  });

  group('AttachmentFile', () {
    Widget buildSubject(List<Widget> Function(BuildContext) children) =>
        Builder(
          builder: (context) {
            return RepositoryProvider<AttachmentsRepository>.value(
              value: attachmentsRepository,
              child: MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
                localizationsDelegates: AppLocalizations.localizationsDelegates,
                home: Scaffold(
                  body: Padding(
                    padding: const EdgeInsets.all(S.s16),
                    child: SizedBox(
                      width: double.infinity,
                      child: Column(
                        spacing: S.s16,
                        crossAxisAlignment: .end,
                        children: children(context),
                      ),
                    ),
                  ),
                ),
              ),
            );
          },
        );

    List<_FileTestBubble> attachmentPerStatus(context) => [
      for (final (color, backgroundColor) in testColors(context))
        for (final attachmentId in testStatuses.keys) ...[
          _FileTestBubble(
            attachmentId: attachmentId,
            color: color,
            backgroundColor: backgroundColor,
          ),
        ],
    ];

    testWidgets('renders correctly', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = physicalSize;
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
      });

      await tester.pumpWidget(buildSubject(attachmentPerStatus));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_file.png'),
      );
    });

    testWidgets('renders correctly (dark mode)', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = physicalSize;
      tester.platformDispatcher.platformBrightnessTestValue = .dark;
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
        tester.platformDispatcher.clearPlatformBrightnessTestValue();
      });

      await tester.pumpWidget(buildSubject(attachmentPerStatus));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_file_dark_mode.png'),
      );
    });

    testWidgets('renders correctly overflow', (tester) async {
      tester.platformDispatcher.views.first.physicalSize = const Size(800, 600);
      addTearDown(() {
        tester.platformDispatcher.views.first.resetPhysicalSize();
      });

      List<_FileTestBubble> children(context) => [
        for (final (color, backgroundColor) in testColors(context))
          _FileTestBubble(
            attachmentId: 5.attachmentId(),
            color: color,
            backgroundColor: backgroundColor,
            attachment: file.copyWith(
              filename: 'a_very_long_filename_which_overflows.bin',
            ),
          ),
      ];

      await tester.pumpWidget(buildSubject(children));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/attachment_file_overflow.png'),
      );
    });

    group('download button of a failed download', () {
      final failedAttachmentId = 3.attachmentId();

      Widget buildFailedAttachment() => buildSubject(
        (context) => [
          AttachmentFile(
            attachment: file.copyWith(attachmentId: failedAttachmentId),
            isSender: false,
            color: Colors.black,
          ),
        ],
      );

      testWidgets('shows the progress once the download is registered', (
        tester,
      ) async {
        final registered = Completer<void>();
        when(
          () => attachmentsRepository.downloadAttachment(
            attachmentId: any(named: 'attachmentId'),
          ),
        ).thenAnswer((_) => registered.future);
        when(
          () => attachmentsRepository.statusStream(
            attachmentId: failedAttachmentId,
          ),
        ).thenAnswer(
          (_) => Stream.value(
            registered.isCompleted
                ? UiAttachmentStatus.progress(BigInt.from(1024))
                : const UiAttachmentStatus.failed(),
          ),
        );

        await tester.pumpWidget(buildFailedAttachment());
        await tester.pumpAndSettle();

        await tester.tap(find.byType(ButtonIcon));
        await tester.pumpAndSettle();
        expect(find.byType(CircularProgressIndicator), findsNothing);

        registered.complete();
        await tester.pumpAndSettle();

        expect(find.byType(CircularProgressIndicator), findsOneWidget);
        verify(
          () => attachmentsRepository.downloadAttachment(
            attachmentId: failedAttachmentId,
          ),
        ).called(1);
      });

      testWidgets('stays when the download fails to start', (tester) async {
        when(
          () => attachmentsRepository.downloadAttachment(
            attachmentId: any(named: 'attachmentId'),
          ),
        ).thenAnswer((_) => Future.error(Exception('download failed')));

        await tester.pumpWidget(buildFailedAttachment());
        await tester.pumpAndSettle();

        await tester.tap(find.byType(ButtonIcon));
        await tester.pumpAndSettle();

        expect(find.byType(ButtonIcon), findsOneWidget);
      });

      testWidgets('is gone before the download is registered', (tester) async {
        final registered = Completer<void>();
        when(
          () => attachmentsRepository.downloadAttachment(
            attachmentId: any(named: 'attachmentId'),
          ),
        ).thenAnswer((_) => registered.future);

        await tester.pumpWidget(buildFailedAttachment());
        await tester.pumpAndSettle();

        await tester.tap(find.byType(ButtonIcon));
        await tester.pumpWidget(const SizedBox());
        registered.complete();
        await tester.pumpAndSettle();

        expect(find.byType(AttachmentFile), findsNothing);
      });
    });
  });
}

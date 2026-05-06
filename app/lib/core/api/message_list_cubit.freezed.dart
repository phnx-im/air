// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'message_list_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$MessageListChange {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListChange);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'MessageListChange()';
}


}

/// @nodoc
class $MessageListChangeCopyWith<$Res>  {
$MessageListChangeCopyWith(MessageListChange _, $Res Function(MessageListChange) __);
}



/// @nodoc


class MessageListChange_Reload extends MessageListChange {
  const MessageListChange_Reload({required final  List<UiChatMessage> messages}): _messages = messages,super._();
  

 final  List<UiChatMessage> _messages;
 List<UiChatMessage> get messages {
  if (_messages is EqualUnmodifiableListView) return _messages;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_messages);
}


/// Create a copy of MessageListChange
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageListChange_ReloadCopyWith<MessageListChange_Reload> get copyWith => _$MessageListChange_ReloadCopyWithImpl<MessageListChange_Reload>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListChange_Reload&&const DeepCollectionEquality().equals(other._messages, _messages));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_messages));

@override
String toString() {
  return 'MessageListChange.reload(messages: $messages)';
}


}

/// @nodoc
abstract mixin class $MessageListChange_ReloadCopyWith<$Res> implements $MessageListChangeCopyWith<$Res> {
  factory $MessageListChange_ReloadCopyWith(MessageListChange_Reload value, $Res Function(MessageListChange_Reload) _then) = _$MessageListChange_ReloadCopyWithImpl;
@useResult
$Res call({
 List<UiChatMessage> messages
});




}
/// @nodoc
class _$MessageListChange_ReloadCopyWithImpl<$Res>
    implements $MessageListChange_ReloadCopyWith<$Res> {
  _$MessageListChange_ReloadCopyWithImpl(this._self, this._then);

  final MessageListChange_Reload _self;
  final $Res Function(MessageListChange_Reload) _then;

/// Create a copy of MessageListChange
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? messages = null,}) {
  return _then(MessageListChange_Reload(
messages: null == messages ? _self._messages : messages // ignore: cast_nullable_to_non_nullable
as List<UiChatMessage>,
  ));
}


}

/// @nodoc


class MessageListChange_Splice extends MessageListChange {
  const MessageListChange_Splice({required this.index, required final  List<UiChatMessage> messages, required this.deleteCount}): _messages = messages,super._();
  

 final  BigInt index;
 final  List<UiChatMessage> _messages;
 List<UiChatMessage> get messages {
  if (_messages is EqualUnmodifiableListView) return _messages;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_messages);
}

 final  BigInt deleteCount;

/// Create a copy of MessageListChange
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageListChange_SpliceCopyWith<MessageListChange_Splice> get copyWith => _$MessageListChange_SpliceCopyWithImpl<MessageListChange_Splice>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListChange_Splice&&(identical(other.index, index) || other.index == index)&&const DeepCollectionEquality().equals(other._messages, _messages)&&(identical(other.deleteCount, deleteCount) || other.deleteCount == deleteCount));
}


@override
int get hashCode => Object.hash(runtimeType,index,const DeepCollectionEquality().hash(_messages),deleteCount);

@override
String toString() {
  return 'MessageListChange.splice(index: $index, messages: $messages, deleteCount: $deleteCount)';
}


}

/// @nodoc
abstract mixin class $MessageListChange_SpliceCopyWith<$Res> implements $MessageListChangeCopyWith<$Res> {
  factory $MessageListChange_SpliceCopyWith(MessageListChange_Splice value, $Res Function(MessageListChange_Splice) _then) = _$MessageListChange_SpliceCopyWithImpl;
@useResult
$Res call({
 BigInt index, List<UiChatMessage> messages, BigInt deleteCount
});




}
/// @nodoc
class _$MessageListChange_SpliceCopyWithImpl<$Res>
    implements $MessageListChange_SpliceCopyWith<$Res> {
  _$MessageListChange_SpliceCopyWithImpl(this._self, this._then);

  final MessageListChange_Splice _self;
  final $Res Function(MessageListChange_Splice) _then;

/// Create a copy of MessageListChange
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? index = null,Object? messages = null,Object? deleteCount = null,}) {
  return _then(MessageListChange_Splice(
index: null == index ? _self.index : index // ignore: cast_nullable_to_non_nullable
as BigInt,messages: null == messages ? _self._messages : messages // ignore: cast_nullable_to_non_nullable
as List<UiChatMessage>,deleteCount: null == deleteCount ? _self.deleteCount : deleteCount // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}

/// @nodoc


class MessageListChange_Patch extends MessageListChange {
  const MessageListChange_Patch({required this.index, required this.message}): super._();
  

 final  BigInt index;
 final  UiChatMessage message;

/// Create a copy of MessageListChange
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageListChange_PatchCopyWith<MessageListChange_Patch> get copyWith => _$MessageListChange_PatchCopyWithImpl<MessageListChange_Patch>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListChange_Patch&&(identical(other.index, index) || other.index == index)&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,index,message);

@override
String toString() {
  return 'MessageListChange.patch(index: $index, message: $message)';
}


}

/// @nodoc
abstract mixin class $MessageListChange_PatchCopyWith<$Res> implements $MessageListChangeCopyWith<$Res> {
  factory $MessageListChange_PatchCopyWith(MessageListChange_Patch value, $Res Function(MessageListChange_Patch) _then) = _$MessageListChange_PatchCopyWithImpl;
@useResult
$Res call({
 BigInt index, UiChatMessage message
});


$UiChatMessageCopyWith<$Res> get message;

}
/// @nodoc
class _$MessageListChange_PatchCopyWithImpl<$Res>
    implements $MessageListChange_PatchCopyWith<$Res> {
  _$MessageListChange_PatchCopyWithImpl(this._self, this._then);

  final MessageListChange_Patch _self;
  final $Res Function(MessageListChange_Patch) _then;

/// Create a copy of MessageListChange
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? index = null,Object? message = null,}) {
  return _then(MessageListChange_Patch(
index: null == index ? _self.index : index // ignore: cast_nullable_to_non_nullable
as BigInt,message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as UiChatMessage,
  ));
}

/// Create a copy of MessageListChange
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$UiChatMessageCopyWith<$Res> get message {
  
  return $UiChatMessageCopyWith<$Res>(_self.message, (value) {
    return _then(_self.copyWith(message: value));
  });
}
}

/// @nodoc
mixin _$MessageListCommand {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListCommand);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'MessageListCommand()';
}


}

/// @nodoc
class $MessageListCommandCopyWith<$Res>  {
$MessageListCommandCopyWith(MessageListCommand _, $Res Function(MessageListCommand) __);
}



/// @nodoc


class MessageListCommand_ScrollToId extends MessageListCommand {
  const MessageListCommand_ScrollToId({required this.messageId}): super._();
  

 final  MessageId messageId;

/// Create a copy of MessageListCommand
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageListCommand_ScrollToIdCopyWith<MessageListCommand_ScrollToId> get copyWith => _$MessageListCommand_ScrollToIdCopyWithImpl<MessageListCommand_ScrollToId>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListCommand_ScrollToId&&(identical(other.messageId, messageId) || other.messageId == messageId));
}


@override
int get hashCode => Object.hash(runtimeType,messageId);

@override
String toString() {
  return 'MessageListCommand.scrollToId(messageId: $messageId)';
}


}

/// @nodoc
abstract mixin class $MessageListCommand_ScrollToIdCopyWith<$Res> implements $MessageListCommandCopyWith<$Res> {
  factory $MessageListCommand_ScrollToIdCopyWith(MessageListCommand_ScrollToId value, $Res Function(MessageListCommand_ScrollToId) _then) = _$MessageListCommand_ScrollToIdCopyWithImpl;
@useResult
$Res call({
 MessageId messageId
});




}
/// @nodoc
class _$MessageListCommand_ScrollToIdCopyWithImpl<$Res>
    implements $MessageListCommand_ScrollToIdCopyWith<$Res> {
  _$MessageListCommand_ScrollToIdCopyWithImpl(this._self, this._then);

  final MessageListCommand_ScrollToId _self;
  final $Res Function(MessageListCommand_ScrollToId) _then;

/// Create a copy of MessageListCommand
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? messageId = null,}) {
  return _then(MessageListCommand_ScrollToId(
messageId: null == messageId ? _self.messageId : messageId // ignore: cast_nullable_to_non_nullable
as MessageId,
  ));
}


}

/// @nodoc


class MessageListCommand_ScrollToBottom extends MessageListCommand {
  const MessageListCommand_ScrollToBottom(): super._();
  






@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListCommand_ScrollToBottom);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'MessageListCommand.scrollToBottom()';
}


}




/// @nodoc
mixin _$MessageListState {

 bool? get isConnectionChat; bool get hasOlder; bool get hasNewer; bool get isAtBottom; int? get firstUnreadIndex; int get revision;
/// Create a copy of MessageListState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageListStateCopyWith<MessageListState> get copyWith => _$MessageListStateCopyWithImpl<MessageListState>(this as MessageListState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListState&&(identical(other.isConnectionChat, isConnectionChat) || other.isConnectionChat == isConnectionChat)&&(identical(other.hasOlder, hasOlder) || other.hasOlder == hasOlder)&&(identical(other.hasNewer, hasNewer) || other.hasNewer == hasNewer)&&(identical(other.isAtBottom, isAtBottom) || other.isAtBottom == isAtBottom)&&(identical(other.firstUnreadIndex, firstUnreadIndex) || other.firstUnreadIndex == firstUnreadIndex)&&(identical(other.revision, revision) || other.revision == revision));
}


@override
int get hashCode => Object.hash(runtimeType,isConnectionChat,hasOlder,hasNewer,isAtBottom,firstUnreadIndex,revision);

@override
String toString() {
  return 'MessageListState(isConnectionChat: $isConnectionChat, hasOlder: $hasOlder, hasNewer: $hasNewer, isAtBottom: $isAtBottom, firstUnreadIndex: $firstUnreadIndex, revision: $revision)';
}


}

/// @nodoc
abstract mixin class $MessageListStateCopyWith<$Res>  {
  factory $MessageListStateCopyWith(MessageListState value, $Res Function(MessageListState) _then) = _$MessageListStateCopyWithImpl;
@useResult
$Res call({
 bool? isConnectionChat, bool hasOlder, bool hasNewer, bool isAtBottom, int? firstUnreadIndex, int revision
});




}
/// @nodoc
class _$MessageListStateCopyWithImpl<$Res>
    implements $MessageListStateCopyWith<$Res> {
  _$MessageListStateCopyWithImpl(this._self, this._then);

  final MessageListState _self;
  final $Res Function(MessageListState) _then;

/// Create a copy of MessageListState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? isConnectionChat = freezed,Object? hasOlder = null,Object? hasNewer = null,Object? isAtBottom = null,Object? firstUnreadIndex = freezed,Object? revision = null,}) {
  return _then(_self.copyWith(
isConnectionChat: freezed == isConnectionChat ? _self.isConnectionChat : isConnectionChat // ignore: cast_nullable_to_non_nullable
as bool?,hasOlder: null == hasOlder ? _self.hasOlder : hasOlder // ignore: cast_nullable_to_non_nullable
as bool,hasNewer: null == hasNewer ? _self.hasNewer : hasNewer // ignore: cast_nullable_to_non_nullable
as bool,isAtBottom: null == isAtBottom ? _self.isAtBottom : isAtBottom // ignore: cast_nullable_to_non_nullable
as bool,firstUnreadIndex: freezed == firstUnreadIndex ? _self.firstUnreadIndex : firstUnreadIndex // ignore: cast_nullable_to_non_nullable
as int?,revision: null == revision ? _self.revision : revision // ignore: cast_nullable_to_non_nullable
as int,
  ));
}

}



/// @nodoc


class _MessageListState extends MessageListState {
  const _MessageListState({this.isConnectionChat, required this.hasOlder, required this.hasNewer, required this.isAtBottom, this.firstUnreadIndex, required this.revision}): super._();
  

@override final  bool? isConnectionChat;
@override final  bool hasOlder;
@override final  bool hasNewer;
@override final  bool isAtBottom;
@override final  int? firstUnreadIndex;
@override final  int revision;

/// Create a copy of MessageListState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$MessageListStateCopyWith<_MessageListState> get copyWith => __$MessageListStateCopyWithImpl<_MessageListState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _MessageListState&&(identical(other.isConnectionChat, isConnectionChat) || other.isConnectionChat == isConnectionChat)&&(identical(other.hasOlder, hasOlder) || other.hasOlder == hasOlder)&&(identical(other.hasNewer, hasNewer) || other.hasNewer == hasNewer)&&(identical(other.isAtBottom, isAtBottom) || other.isAtBottom == isAtBottom)&&(identical(other.firstUnreadIndex, firstUnreadIndex) || other.firstUnreadIndex == firstUnreadIndex)&&(identical(other.revision, revision) || other.revision == revision));
}


@override
int get hashCode => Object.hash(runtimeType,isConnectionChat,hasOlder,hasNewer,isAtBottom,firstUnreadIndex,revision);

@override
String toString() {
  return 'MessageListState(isConnectionChat: $isConnectionChat, hasOlder: $hasOlder, hasNewer: $hasNewer, isAtBottom: $isAtBottom, firstUnreadIndex: $firstUnreadIndex, revision: $revision)';
}


}

/// @nodoc
abstract mixin class _$MessageListStateCopyWith<$Res> implements $MessageListStateCopyWith<$Res> {
  factory _$MessageListStateCopyWith(_MessageListState value, $Res Function(_MessageListState) _then) = __$MessageListStateCopyWithImpl;
@override @useResult
$Res call({
 bool? isConnectionChat, bool hasOlder, bool hasNewer, bool isAtBottom, int? firstUnreadIndex, int revision
});




}
/// @nodoc
class __$MessageListStateCopyWithImpl<$Res>
    implements _$MessageListStateCopyWith<$Res> {
  __$MessageListStateCopyWithImpl(this._self, this._then);

  final _MessageListState _self;
  final $Res Function(_MessageListState) _then;

/// Create a copy of MessageListState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? isConnectionChat = freezed,Object? hasOlder = null,Object? hasNewer = null,Object? isAtBottom = null,Object? firstUnreadIndex = freezed,Object? revision = null,}) {
  return _then(_MessageListState(
isConnectionChat: freezed == isConnectionChat ? _self.isConnectionChat : isConnectionChat // ignore: cast_nullable_to_non_nullable
as bool?,hasOlder: null == hasOlder ? _self.hasOlder : hasOlder // ignore: cast_nullable_to_non_nullable
as bool,hasNewer: null == hasNewer ? _self.hasNewer : hasNewer // ignore: cast_nullable_to_non_nullable
as bool,isAtBottom: null == isAtBottom ? _self.isAtBottom : isAtBottom // ignore: cast_nullable_to_non_nullable
as bool,firstUnreadIndex: freezed == firstUnreadIndex ? _self.firstUnreadIndex : firstUnreadIndex // ignore: cast_nullable_to_non_nullable
as int?,revision: null == revision ? _self.revision : revision // ignore: cast_nullable_to_non_nullable
as int,
  ));
}


}

/// @nodoc
mixin _$MessageListTransition {

 int get revision; MessageListTransitionKind get kind; List<MessageListChange> get changes; MessageListCommand? get command;
/// Create a copy of MessageListTransition
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageListTransitionCopyWith<MessageListTransition> get copyWith => _$MessageListTransitionCopyWithImpl<MessageListTransition>(this as MessageListTransition, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListTransition&&(identical(other.revision, revision) || other.revision == revision)&&(identical(other.kind, kind) || other.kind == kind)&&const DeepCollectionEquality().equals(other.changes, changes)&&(identical(other.command, command) || other.command == command));
}


@override
int get hashCode => Object.hash(runtimeType,revision,kind,const DeepCollectionEquality().hash(changes),command);

@override
String toString() {
  return 'MessageListTransition(revision: $revision, kind: $kind, changes: $changes, command: $command)';
}


}

/// @nodoc
abstract mixin class $MessageListTransitionCopyWith<$Res>  {
  factory $MessageListTransitionCopyWith(MessageListTransition value, $Res Function(MessageListTransition) _then) = _$MessageListTransitionCopyWithImpl;
@useResult
$Res call({
 int revision, MessageListTransitionKind kind, List<MessageListChange> changes, MessageListCommand? command
});


$MessageListCommandCopyWith<$Res>? get command;

}
/// @nodoc
class _$MessageListTransitionCopyWithImpl<$Res>
    implements $MessageListTransitionCopyWith<$Res> {
  _$MessageListTransitionCopyWithImpl(this._self, this._then);

  final MessageListTransition _self;
  final $Res Function(MessageListTransition) _then;

/// Create a copy of MessageListTransition
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? revision = null,Object? kind = null,Object? changes = null,Object? command = freezed,}) {
  return _then(_self.copyWith(
revision: null == revision ? _self.revision : revision // ignore: cast_nullable_to_non_nullable
as int,kind: null == kind ? _self.kind : kind // ignore: cast_nullable_to_non_nullable
as MessageListTransitionKind,changes: null == changes ? _self.changes : changes // ignore: cast_nullable_to_non_nullable
as List<MessageListChange>,command: freezed == command ? _self.command : command // ignore: cast_nullable_to_non_nullable
as MessageListCommand?,
  ));
}
/// Create a copy of MessageListTransition
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$MessageListCommandCopyWith<$Res>? get command {
    if (_self.command == null) {
    return null;
  }

  return $MessageListCommandCopyWith<$Res>(_self.command!, (value) {
    return _then(_self.copyWith(command: value));
  });
}
}



/// @nodoc


class _MessageListTransition implements MessageListTransition {
  const _MessageListTransition({required this.revision, required this.kind, required final  List<MessageListChange> changes, this.command}): _changes = changes;
  

@override final  int revision;
@override final  MessageListTransitionKind kind;
 final  List<MessageListChange> _changes;
@override List<MessageListChange> get changes {
  if (_changes is EqualUnmodifiableListView) return _changes;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_changes);
}

@override final  MessageListCommand? command;

/// Create a copy of MessageListTransition
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$MessageListTransitionCopyWith<_MessageListTransition> get copyWith => __$MessageListTransitionCopyWithImpl<_MessageListTransition>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _MessageListTransition&&(identical(other.revision, revision) || other.revision == revision)&&(identical(other.kind, kind) || other.kind == kind)&&const DeepCollectionEquality().equals(other._changes, _changes)&&(identical(other.command, command) || other.command == command));
}


@override
int get hashCode => Object.hash(runtimeType,revision,kind,const DeepCollectionEquality().hash(_changes),command);

@override
String toString() {
  return 'MessageListTransition(revision: $revision, kind: $kind, changes: $changes, command: $command)';
}


}

/// @nodoc
abstract mixin class _$MessageListTransitionCopyWith<$Res> implements $MessageListTransitionCopyWith<$Res> {
  factory _$MessageListTransitionCopyWith(_MessageListTransition value, $Res Function(_MessageListTransition) _then) = __$MessageListTransitionCopyWithImpl;
@override @useResult
$Res call({
 int revision, MessageListTransitionKind kind, List<MessageListChange> changes, MessageListCommand? command
});


@override $MessageListCommandCopyWith<$Res>? get command;

}
/// @nodoc
class __$MessageListTransitionCopyWithImpl<$Res>
    implements _$MessageListTransitionCopyWith<$Res> {
  __$MessageListTransitionCopyWithImpl(this._self, this._then);

  final _MessageListTransition _self;
  final $Res Function(_MessageListTransition) _then;

/// Create a copy of MessageListTransition
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? revision = null,Object? kind = null,Object? changes = null,Object? command = freezed,}) {
  return _then(_MessageListTransition(
revision: null == revision ? _self.revision : revision // ignore: cast_nullable_to_non_nullable
as int,kind: null == kind ? _self.kind : kind // ignore: cast_nullable_to_non_nullable
as MessageListTransitionKind,changes: null == changes ? _self._changes : changes // ignore: cast_nullable_to_non_nullable
as List<MessageListChange>,command: freezed == command ? _self.command : command // ignore: cast_nullable_to_non_nullable
as MessageListCommand?,
  ));
}

/// Create a copy of MessageListTransition
/// with the given fields replaced by the non-null parameter values.
@override
@pragma('vm:prefer-inline')
$MessageListCommandCopyWith<$Res>? get command {
    if (_self.command == null) {
    return null;
  }

  return $MessageListCommandCopyWith<$Res>(_self.command!, (value) {
    return _then(_self.copyWith(command: value));
  });
}
}

// dart format on

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
mixin _$MessageListDiff {





@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListDiff);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
  return 'MessageListDiff()';
}


}

/// @nodoc
class $MessageListDiffCopyWith<$Res>  {
$MessageListDiffCopyWith(MessageListDiff _, $Res Function(MessageListDiff) __);
}



/// @nodoc


class MessageListDiff_Insert extends MessageListDiff {
  const MessageListDiff_Insert({required this.index, required final  List<UiChatMessage> messages}): _messages = messages,super._();
  

 final  BigInt index;
 final  List<UiChatMessage> _messages;
 List<UiChatMessage> get messages {
  if (_messages is EqualUnmodifiableListView) return _messages;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_messages);
}


/// Create a copy of MessageListDiff
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageListDiff_InsertCopyWith<MessageListDiff_Insert> get copyWith => _$MessageListDiff_InsertCopyWithImpl<MessageListDiff_Insert>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListDiff_Insert&&(identical(other.index, index) || other.index == index)&&const DeepCollectionEquality().equals(other._messages, _messages));
}


@override
int get hashCode => Object.hash(runtimeType,index,const DeepCollectionEquality().hash(_messages));

@override
String toString() {
  return 'MessageListDiff.insert(index: $index, messages: $messages)';
}


}

/// @nodoc
abstract mixin class $MessageListDiff_InsertCopyWith<$Res> implements $MessageListDiffCopyWith<$Res> {
  factory $MessageListDiff_InsertCopyWith(MessageListDiff_Insert value, $Res Function(MessageListDiff_Insert) _then) = _$MessageListDiff_InsertCopyWithImpl;
@useResult
$Res call({
 BigInt index, List<UiChatMessage> messages
});




}
/// @nodoc
class _$MessageListDiff_InsertCopyWithImpl<$Res>
    implements $MessageListDiff_InsertCopyWith<$Res> {
  _$MessageListDiff_InsertCopyWithImpl(this._self, this._then);

  final MessageListDiff_Insert _self;
  final $Res Function(MessageListDiff_Insert) _then;

/// Create a copy of MessageListDiff
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? index = null,Object? messages = null,}) {
  return _then(MessageListDiff_Insert(
index: null == index ? _self.index : index // ignore: cast_nullable_to_non_nullable
as BigInt,messages: null == messages ? _self._messages : messages // ignore: cast_nullable_to_non_nullable
as List<UiChatMessage>,
  ));
}


}

/// @nodoc


class MessageListDiff_Remove extends MessageListDiff {
  const MessageListDiff_Remove({required this.index, required this.count}): super._();
  

 final  BigInt index;
 final  BigInt count;

/// Create a copy of MessageListDiff
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageListDiff_RemoveCopyWith<MessageListDiff_Remove> get copyWith => _$MessageListDiff_RemoveCopyWithImpl<MessageListDiff_Remove>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListDiff_Remove&&(identical(other.index, index) || other.index == index)&&(identical(other.count, count) || other.count == count));
}


@override
int get hashCode => Object.hash(runtimeType,index,count);

@override
String toString() {
  return 'MessageListDiff.remove(index: $index, count: $count)';
}


}

/// @nodoc
abstract mixin class $MessageListDiff_RemoveCopyWith<$Res> implements $MessageListDiffCopyWith<$Res> {
  factory $MessageListDiff_RemoveCopyWith(MessageListDiff_Remove value, $Res Function(MessageListDiff_Remove) _then) = _$MessageListDiff_RemoveCopyWithImpl;
@useResult
$Res call({
 BigInt index, BigInt count
});




}
/// @nodoc
class _$MessageListDiff_RemoveCopyWithImpl<$Res>
    implements $MessageListDiff_RemoveCopyWith<$Res> {
  _$MessageListDiff_RemoveCopyWithImpl(this._self, this._then);

  final MessageListDiff_Remove _self;
  final $Res Function(MessageListDiff_Remove) _then;

/// Create a copy of MessageListDiff
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? index = null,Object? count = null,}) {
  return _then(MessageListDiff_Remove(
index: null == index ? _self.index : index // ignore: cast_nullable_to_non_nullable
as BigInt,count: null == count ? _self.count : count // ignore: cast_nullable_to_non_nullable
as BigInt,
  ));
}


}

/// @nodoc


class MessageListDiff_Update extends MessageListDiff {
  const MessageListDiff_Update({required this.index, required this.message}): super._();
  

 final  BigInt index;
 final  UiChatMessage message;

/// Create a copy of MessageListDiff
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageListDiff_UpdateCopyWith<MessageListDiff_Update> get copyWith => _$MessageListDiff_UpdateCopyWithImpl<MessageListDiff_Update>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListDiff_Update&&(identical(other.index, index) || other.index == index)&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode => Object.hash(runtimeType,index,message);

@override
String toString() {
  return 'MessageListDiff.update(index: $index, message: $message)';
}


}

/// @nodoc
abstract mixin class $MessageListDiff_UpdateCopyWith<$Res> implements $MessageListDiffCopyWith<$Res> {
  factory $MessageListDiff_UpdateCopyWith(MessageListDiff_Update value, $Res Function(MessageListDiff_Update) _then) = _$MessageListDiff_UpdateCopyWithImpl;
@useResult
$Res call({
 BigInt index, UiChatMessage message
});


$UiChatMessageCopyWith<$Res> get message;

}
/// @nodoc
class _$MessageListDiff_UpdateCopyWithImpl<$Res>
    implements $MessageListDiff_UpdateCopyWith<$Res> {
  _$MessageListDiff_UpdateCopyWithImpl(this._self, this._then);

  final MessageListDiff_Update _self;
  final $Res Function(MessageListDiff_Update) _then;

/// Create a copy of MessageListDiff
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? index = null,Object? message = null,}) {
  return _then(MessageListDiff_Update(
index: null == index ? _self.index : index // ignore: cast_nullable_to_non_nullable
as BigInt,message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as UiChatMessage,
  ));
}

/// Create a copy of MessageListDiff
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


class MessageListDiff_Reload extends MessageListDiff {
  const MessageListDiff_Reload({required final  List<UiChatMessage> messages}): _messages = messages,super._();
  

 final  List<UiChatMessage> _messages;
 List<UiChatMessage> get messages {
  if (_messages is EqualUnmodifiableListView) return _messages;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_messages);
}


/// Create a copy of MessageListDiff
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageListDiff_ReloadCopyWith<MessageListDiff_Reload> get copyWith => _$MessageListDiff_ReloadCopyWithImpl<MessageListDiff_Reload>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListDiff_Reload&&const DeepCollectionEquality().equals(other._messages, _messages));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_messages));

@override
String toString() {
  return 'MessageListDiff.reload(messages: $messages)';
}


}

/// @nodoc
abstract mixin class $MessageListDiff_ReloadCopyWith<$Res> implements $MessageListDiffCopyWith<$Res> {
  factory $MessageListDiff_ReloadCopyWith(MessageListDiff_Reload value, $Res Function(MessageListDiff_Reload) _then) = _$MessageListDiff_ReloadCopyWithImpl;
@useResult
$Res call({
 List<UiChatMessage> messages
});




}
/// @nodoc
class _$MessageListDiff_ReloadCopyWithImpl<$Res>
    implements $MessageListDiff_ReloadCopyWith<$Res> {
  _$MessageListDiff_ReloadCopyWithImpl(this._self, this._then);

  final MessageListDiff_Reload _self;
  final $Res Function(MessageListDiff_Reload) _then;

/// Create a copy of MessageListDiff
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? messages = null,}) {
  return _then(MessageListDiff_Reload(
messages: null == messages ? _self._messages : messages // ignore: cast_nullable_to_non_nullable
as List<UiChatMessage>,
  ));
}


}

/// @nodoc
mixin _$MessageListMeta {

 bool? get isConnectionChat; bool get hasOlder; bool get hasNewer; bool get isAtBottom; int? get scrollToIndex; int? get firstUnreadIndex;
/// Create a copy of MessageListMeta
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$MessageListMetaCopyWith<MessageListMeta> get copyWith => _$MessageListMetaCopyWithImpl<MessageListMeta>(this as MessageListMeta, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is MessageListMeta&&(identical(other.isConnectionChat, isConnectionChat) || other.isConnectionChat == isConnectionChat)&&(identical(other.hasOlder, hasOlder) || other.hasOlder == hasOlder)&&(identical(other.hasNewer, hasNewer) || other.hasNewer == hasNewer)&&(identical(other.isAtBottom, isAtBottom) || other.isAtBottom == isAtBottom)&&(identical(other.scrollToIndex, scrollToIndex) || other.scrollToIndex == scrollToIndex)&&(identical(other.firstUnreadIndex, firstUnreadIndex) || other.firstUnreadIndex == firstUnreadIndex));
}


@override
int get hashCode => Object.hash(runtimeType,isConnectionChat,hasOlder,hasNewer,isAtBottom,scrollToIndex,firstUnreadIndex);

@override
String toString() {
  return 'MessageListMeta(isConnectionChat: $isConnectionChat, hasOlder: $hasOlder, hasNewer: $hasNewer, isAtBottom: $isAtBottom, scrollToIndex: $scrollToIndex, firstUnreadIndex: $firstUnreadIndex)';
}


}

/// @nodoc
abstract mixin class $MessageListMetaCopyWith<$Res>  {
  factory $MessageListMetaCopyWith(MessageListMeta value, $Res Function(MessageListMeta) _then) = _$MessageListMetaCopyWithImpl;
@useResult
$Res call({
 bool? isConnectionChat, bool hasOlder, bool hasNewer, bool isAtBottom, int? scrollToIndex, int? firstUnreadIndex
});




}
/// @nodoc
class _$MessageListMetaCopyWithImpl<$Res>
    implements $MessageListMetaCopyWith<$Res> {
  _$MessageListMetaCopyWithImpl(this._self, this._then);

  final MessageListMeta _self;
  final $Res Function(MessageListMeta) _then;

/// Create a copy of MessageListMeta
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? isConnectionChat = freezed,Object? hasOlder = null,Object? hasNewer = null,Object? isAtBottom = null,Object? scrollToIndex = freezed,Object? firstUnreadIndex = freezed,}) {
  return _then(_self.copyWith(
isConnectionChat: freezed == isConnectionChat ? _self.isConnectionChat : isConnectionChat // ignore: cast_nullable_to_non_nullable
as bool?,hasOlder: null == hasOlder ? _self.hasOlder : hasOlder // ignore: cast_nullable_to_non_nullable
as bool,hasNewer: null == hasNewer ? _self.hasNewer : hasNewer // ignore: cast_nullable_to_non_nullable
as bool,isAtBottom: null == isAtBottom ? _self.isAtBottom : isAtBottom // ignore: cast_nullable_to_non_nullable
as bool,scrollToIndex: freezed == scrollToIndex ? _self.scrollToIndex : scrollToIndex // ignore: cast_nullable_to_non_nullable
as int?,firstUnreadIndex: freezed == firstUnreadIndex ? _self.firstUnreadIndex : firstUnreadIndex // ignore: cast_nullable_to_non_nullable
as int?,
  ));
}

}



/// @nodoc


class _MessageListMeta extends MessageListMeta {
  const _MessageListMeta({this.isConnectionChat, required this.hasOlder, required this.hasNewer, required this.isAtBottom, this.scrollToIndex, this.firstUnreadIndex}): super._();
  

@override final  bool? isConnectionChat;
@override final  bool hasOlder;
@override final  bool hasNewer;
@override final  bool isAtBottom;
@override final  int? scrollToIndex;
@override final  int? firstUnreadIndex;

/// Create a copy of MessageListMeta
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$MessageListMetaCopyWith<_MessageListMeta> get copyWith => __$MessageListMetaCopyWithImpl<_MessageListMeta>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _MessageListMeta&&(identical(other.isConnectionChat, isConnectionChat) || other.isConnectionChat == isConnectionChat)&&(identical(other.hasOlder, hasOlder) || other.hasOlder == hasOlder)&&(identical(other.hasNewer, hasNewer) || other.hasNewer == hasNewer)&&(identical(other.isAtBottom, isAtBottom) || other.isAtBottom == isAtBottom)&&(identical(other.scrollToIndex, scrollToIndex) || other.scrollToIndex == scrollToIndex)&&(identical(other.firstUnreadIndex, firstUnreadIndex) || other.firstUnreadIndex == firstUnreadIndex));
}


@override
int get hashCode => Object.hash(runtimeType,isConnectionChat,hasOlder,hasNewer,isAtBottom,scrollToIndex,firstUnreadIndex);

@override
String toString() {
  return 'MessageListMeta(isConnectionChat: $isConnectionChat, hasOlder: $hasOlder, hasNewer: $hasNewer, isAtBottom: $isAtBottom, scrollToIndex: $scrollToIndex, firstUnreadIndex: $firstUnreadIndex)';
}


}

/// @nodoc
abstract mixin class _$MessageListMetaCopyWith<$Res> implements $MessageListMetaCopyWith<$Res> {
  factory _$MessageListMetaCopyWith(_MessageListMeta value, $Res Function(_MessageListMeta) _then) = __$MessageListMetaCopyWithImpl;
@override @useResult
$Res call({
 bool? isConnectionChat, bool hasOlder, bool hasNewer, bool isAtBottom, int? scrollToIndex, int? firstUnreadIndex
});




}
/// @nodoc
class __$MessageListMetaCopyWithImpl<$Res>
    implements _$MessageListMetaCopyWith<$Res> {
  __$MessageListMetaCopyWithImpl(this._self, this._then);

  final _MessageListMeta _self;
  final $Res Function(_MessageListMeta) _then;

/// Create a copy of MessageListMeta
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? isConnectionChat = freezed,Object? hasOlder = null,Object? hasNewer = null,Object? isAtBottom = null,Object? scrollToIndex = freezed,Object? firstUnreadIndex = freezed,}) {
  return _then(_MessageListMeta(
isConnectionChat: freezed == isConnectionChat ? _self.isConnectionChat : isConnectionChat // ignore: cast_nullable_to_non_nullable
as bool?,hasOlder: null == hasOlder ? _self.hasOlder : hasOlder // ignore: cast_nullable_to_non_nullable
as bool,hasNewer: null == hasNewer ? _self.hasNewer : hasNewer // ignore: cast_nullable_to_non_nullable
as bool,isAtBottom: null == isAtBottom ? _self.isAtBottom : isAtBottom // ignore: cast_nullable_to_non_nullable
as bool,scrollToIndex: freezed == scrollToIndex ? _self.scrollToIndex : scrollToIndex // ignore: cast_nullable_to_non_nullable
as int?,firstUnreadIndex: freezed == firstUnreadIndex ? _self.firstUnreadIndex : firstUnreadIndex // ignore: cast_nullable_to_non_nullable
as int?,
  ));
}


}

// dart format on

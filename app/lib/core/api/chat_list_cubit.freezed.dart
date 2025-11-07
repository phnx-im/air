// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'chat_list_cubit.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$ChatListState {

 List<ChatId> get chatIds;
/// Create a copy of ChatListState
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$ChatListStateCopyWith<ChatListState> get copyWith => _$ChatListStateCopyWithImpl<ChatListState>(this as ChatListState, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is ChatListState&&const DeepCollectionEquality().equals(other.chatIds, chatIds));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(chatIds));

@override
String toString() {
  return 'ChatListState(chatIds: $chatIds)';
}


}

/// @nodoc
abstract mixin class $ChatListStateCopyWith<$Res>  {
  factory $ChatListStateCopyWith(ChatListState value, $Res Function(ChatListState) _then) = _$ChatListStateCopyWithImpl;
@useResult
$Res call({
 List<ChatId> chatIds
});




}
/// @nodoc
class _$ChatListStateCopyWithImpl<$Res>
    implements $ChatListStateCopyWith<$Res> {
  _$ChatListStateCopyWithImpl(this._self, this._then);

  final ChatListState _self;
  final $Res Function(ChatListState) _then;

/// Create a copy of ChatListState
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') @override $Res call({Object? chatIds = null,}) {
  return _then(_self.copyWith(
chatIds: null == chatIds ? _self.chatIds : chatIds // ignore: cast_nullable_to_non_nullable
as List<ChatId>,
  ));
}

}



/// @nodoc


class _ChatListState extends ChatListState {
  const _ChatListState({required final  List<ChatId> chatIds}): _chatIds = chatIds,super._();
  

 final  List<ChatId> _chatIds;
@override List<ChatId> get chatIds {
  if (_chatIds is EqualUnmodifiableListView) return _chatIds;
  // ignore: implicit_dynamic_type
  return EqualUnmodifiableListView(_chatIds);
}


/// Create a copy of ChatListState
/// with the given fields replaced by the non-null parameter values.
@override @JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
_$ChatListStateCopyWith<_ChatListState> get copyWith => __$ChatListStateCopyWithImpl<_ChatListState>(this, _$identity);



@override
bool operator ==(Object other) {
  return identical(this, other) || (other.runtimeType == runtimeType&&other is _ChatListState&&const DeepCollectionEquality().equals(other._chatIds, _chatIds));
}


@override
int get hashCode => Object.hash(runtimeType,const DeepCollectionEquality().hash(_chatIds));

@override
String toString() {
  return 'ChatListState(chatIds: $chatIds)';
}


}

/// @nodoc
abstract mixin class _$ChatListStateCopyWith<$Res> implements $ChatListStateCopyWith<$Res> {
  factory _$ChatListStateCopyWith(_ChatListState value, $Res Function(_ChatListState) _then) = __$ChatListStateCopyWithImpl;
@override @useResult
$Res call({
 List<ChatId> chatIds
});




}
/// @nodoc
class __$ChatListStateCopyWithImpl<$Res>
    implements _$ChatListStateCopyWith<$Res> {
  __$ChatListStateCopyWithImpl(this._self, this._then);

  final _ChatListState _self;
  final $Res Function(_ChatListState) _then;

/// Create a copy of ChatListState
/// with the given fields replaced by the non-null parameter values.
@override @pragma('vm:prefer-inline') $Res call({Object? chatIds = null,}) {
  return _then(_ChatListState(
chatIds: null == chatIds ? _self._chatIds : chatIds // ignore: cast_nullable_to_non_nullable
as List<ChatId>,
  ));
}


}

// dart format on

-module(gleam@otp@actor).
-compile([no_auto_import, nowarn_unused_vars]).

-export([to_erlang_start_result/1, send/2, call/3, start_spec/1, start/2]).
-export_type([message/1, next/1, init_result/2, self/2, spec/2, start_error/0, start_init_message/1]).

-type message(FIS) :: {message, FIS} |
    {system, gleam@otp@system:system_message()} |
    {unexpected, gleam@dynamic:dynamic()}.

-type next(FIT) :: {continue, FIT} | {stop, gleam@erlang@process:exit_reason()}.

-type init_result(FIU, FIV) :: {ready, FIU, gleam@erlang@process:selector(FIV)} |
    {failed, binary()}.

-type self(FIW, FIX) :: {self,
        gleam@otp@system:mode(),
        gleam@erlang@process:pid_(),
        FIW,
        gleam@erlang@process:selector(message(FIX)),
        gleam@otp@system:debug_state(),
        fun((FIX, FIW) -> next(FIW))}.

-type spec(FIY, FIZ) :: {spec,
        fun(() -> init_result(FIY, FIZ)),
        integer(),
        fun((FIZ, FIY) -> next(FIY))}.

-type start_error() :: init_timeout |
    {init_failed, gleam@erlang@process:exit_reason()} |
    {init_crashed, gleam@dynamic:dynamic()}.

-type start_init_message(FJA) :: {ack,
        {ok, gleam@erlang@process:subject(FJA)} |
            {error, gleam@erlang@process:exit_reason()}} |
    {mon, gleam@erlang@process:process_down()}.

-spec exit_process(gleam@erlang@process:exit_reason()) -> gleam@erlang@process:exit_reason().
exit_process(Reason) ->
    Reason.

-spec process_status_info(self(any(), any())) -> gleam@otp@system:status_info().
process_status_info(Self) ->
    {status_info,
        erlang:binary_to_atom(<<"gleam@otp@actor"/utf8>>),
        erlang:element(3, Self),
        erlang:element(2, Self),
        erlang:element(6, Self),
        gleam@dynamic:from(erlang:element(4, Self))}.

-spec to_erlang_start_result(
    {ok, gleam@erlang@process:subject(any())} | {error, start_error()}
) -> {ok, gleam@erlang@process:pid_()} | {error, gleam@dynamic:dynamic()}.
to_erlang_start_result(Res) ->
    case Res of
        {ok, X} ->
            {ok, gleam@erlang@process:subject_owner(X)};

        {error, X@1} ->
            {error, gleam@dynamic:from(X@1)}
    end.

-spec send(gleam@erlang@process:subject(FKX), FKX) -> nil.
send(Subject, Msg) ->
    gleam@erlang@process:send(Subject, Msg).

-spec call(
    gleam@erlang@process:subject(FKZ),
    fun((gleam@erlang@process:subject(FLB)) -> FKZ),
    integer()
) -> FLB.
call(Selector, Make_message, Timeout) ->
    gleam@erlang@process:call(Selector, Make_message, Timeout).

-spec selecting_system_messages(gleam@erlang@process:selector(message(FJM))) -> gleam@erlang@process:selector(message(FJM)).
selecting_system_messages(Selector) ->
    _pipe = Selector,
    gleam@erlang@process:selecting_record3(
        _pipe,
        erlang:binary_to_atom(<<"system"/utf8>>),
        fun gleam_otp_external:convert_system_message/2
    ).

-spec receive_message(self(any(), FJI)) -> message(FJI).
receive_message(Self) ->
    Selector = case erlang:element(2, Self) of
        suspended ->
            _pipe = gleam_erlang_ffi:new_selector(),
            selecting_system_messages(_pipe);

        running ->
            _pipe@1 = gleam_erlang_ffi:new_selector(),
            _pipe@2 = gleam@erlang@process:selecting_anything(
                _pipe@1,
                fun(Field@0) -> {unexpected, Field@0} end
            ),
            _pipe@3 = gleam_erlang_ffi:merge_selector(
                _pipe@2,
                erlang:element(5, Self)
            ),
            selecting_system_messages(_pipe@3)
    end,
    gleam_erlang_ffi:select(Selector).

-spec loop(self(any(), any())) -> gleam@erlang@process:exit_reason().
loop(Self) ->
    case receive_message(Self) of
        {system, System} ->
            case System of
                {get_state, Callback} ->
                    Callback(gleam@dynamic:from(erlang:element(4, Self))),
                    loop(Self);

                {resume, Callback@1} ->
                    Callback@1(),
                    loop(erlang:setelement(2, Self, running));

                {suspend, Callback@2} ->
                    Callback@2(),
                    loop(erlang:setelement(2, Self, suspended));

                {get_status, Callback@3} ->
                    Callback@3(process_status_info(Self)),
                    loop(Self)
            end;

        {unexpected, Message} ->
            logger:warning(
                unicode:characters_to_list(
                    <<"Actor discarding unexpected message: ~s"/utf8>>
                ),
                [unicode:characters_to_list(gleam@string:inspect(Message))]
            ),
            loop(Self);

        {message, Msg} ->
            case (erlang:element(7, Self))(Msg, erlang:element(4, Self)) of
                {stop, Reason} ->
                    exit_process(Reason);

                {continue, State} ->
                    loop(erlang:setelement(4, Self, State))
            end
    end.

-spec initialise_actor(
    spec(any(), FKA),
    gleam@erlang@process:subject({ok, gleam@erlang@process:subject(FKA)} |
        {error, gleam@erlang@process:exit_reason()})
) -> gleam@erlang@process:exit_reason().
initialise_actor(Spec, Ack) ->
    Subject = gleam@erlang@process:new_subject(),
    case (erlang:element(2, Spec))() of
        {ready, State, Selector} ->
            Selector@1 = begin
                _pipe = gleam_erlang_ffi:new_selector(),
                _pipe@1 = gleam@erlang@process:selecting(
                    _pipe,
                    Subject,
                    fun(Field@0) -> {message, Field@0} end
                ),
                gleam_erlang_ffi:merge_selector(
                    _pipe@1,
                    gleam_erlang_ffi:map_selector(
                        Selector,
                        fun(Field@0) -> {message, Field@0} end
                    )
                )
            end,
            gleam@erlang@process:send(Ack, {ok, Subject}),
            Self = {self,
                running,
                gleam@erlang@process:subject_owner(Ack),
                State,
                Selector@1,
                sys:debug_options([]),
                erlang:element(4, Spec)},
            loop(Self);

        {failed, Reason} ->
            gleam@erlang@process:send(Ack, {error, {abnormal, Reason}}),
            exit_process({abnormal, Reason})
    end.

-spec start_spec(spec(any(), FKL)) -> {ok, gleam@erlang@process:subject(FKL)} |
    {error, start_error()}.
start_spec(Spec) ->
    Ack_subject = gleam@erlang@process:new_subject(),
    Child = gleam@erlang@process:start(
        fun() -> initialise_actor(Spec, Ack_subject) end,
        true
    ),
    Monitor = gleam@erlang@process:monitor_process(Child),
    Selector = begin
        _pipe = gleam_erlang_ffi:new_selector(),
        _pipe@1 = gleam@erlang@process:selecting(
            _pipe,
            Ack_subject,
            fun(Field@0) -> {ack, Field@0} end
        ),
        gleam@erlang@process:selecting_process_down(
            _pipe@1,
            Monitor,
            fun(Field@0) -> {mon, Field@0} end
        )
    end,
    Result = case gleam_erlang_ffi:select(Selector, erlang:element(3, Spec)) of
        {ok, {ack, {ok, Channel}}} ->
            {ok, Channel};

        {ok, {ack, {error, Reason}}} ->
            {error, {init_failed, Reason}};

        {ok, {mon, Down}} ->
            {error, {init_crashed, erlang:element(3, Down)}};

        {error, nil} ->
            gleam@erlang@process:kill(Child),
            {error, init_timeout}
    end,
    gleam_erlang_ffi:demonitor(Monitor),
    Result.

-spec start(FKR, fun((FKS, FKR) -> next(FKR))) -> {ok,
        gleam@erlang@process:subject(FKS)} |
    {error, start_error()}.
start(State, Loop) ->
    start_spec(
        {spec,
            fun() -> {ready, State, gleam_erlang_ffi:new_selector()} end,
            5000,
            Loop}
    ).

create table event_update_valid_block
(
    update_block bigint not null,
    donor text not null,
    end_block bigint not null,
    block_price_gwei bigint not null,
    cid text not null,
    ts timestamp without time zone,
    primary key (update_block, donor, end_block, block_price_gwei, cid)
);
create table event_update_valid_block
(
    chain_id bigint not null,
    cid text not null,
    donor text not null,
    update_block bigint not null,
    end_block bigint not null,
    block_price_gwei bigint not null,
    ts timestamp without time zone,
    primary key (update_block, donor, end_block, block_price_gwei, cid, chain_id)
);

create table pinned_cids
(
    chain_id bigint not null,
    node text NOT NULL,
    cid TEXT NOT NULL,
    end_block BIGINT NOT NULL,
    donor TEXT NOT NULL,
    primary key (chain_id, node, cid, end_block, donor)
);

create table failed_pins
(
    chain_id bigint not null,
    node text NOT NULL,
    cid TEXT NOT NULL,
    end_block BIGINT NOT NULL,
    donor TEXT NOT NULL,
    primary key (chain_id, node, cid, end_block, donor)
);

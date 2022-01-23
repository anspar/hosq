drop table if exists event_update_valid_block;
create table event_update_valid_block
(
    chain_id bigint not null,
    cid text not null,
    donor text not null,
    update_block bigint not null,
    end_block bigint not null,
    manual_add boolean default false,
    -- provider_id bigint not null,
    -- ts timestamp without time zone,
    primary key (update_block, donor, end_block, cid, chain_id, manual_add)
);

drop table if exists event_add_provider;
create table event_add_provider
(
    chain_id bigint not null,
    update_block bigint not null,
    owner text not null,
    provider_id bigint not null,
    block_price bigint not null,
    api_url text not null,
    primary key (chain_id, update_block, owner, provider_id, block_price, api_url)
);

drop table if exists pinned_cids;
create table pinned_cids
(
    chain_id bigint not null,
    node text NOT NULL,
    cid TEXT NOT NULL,
    end_block BIGINT NOT NULL,
    primary key (chain_id, node, cid, end_block)
);

drop table if exists failed_pins;
create table failed_pins
(
    chain_id bigint not null,
    node text NOT NULL,
    cid TEXT NOT NULL,
    end_block BIGINT NOT NULL,
    primary key (chain_id, node, cid, end_block)
);

use crate::types::{db::EventUpdateValidBlock, CIDInfo};

pub fn add_valid_block(client: &mut postgres::Client, event: EventUpdateValidBlock)->Result<u64, postgres::Error>{
    client.execute("
                    INSERT INTO event_update_valid_block ( donor, update_block, end_block, cid, chain_id, manual_add) 
                    values ( LOWER($1::TEXT), $2::BIGINT, $3::BIGINT, $4::TEXT, $5::BIGINT, $6::BOOLEAN)
                    ON CONFLICT (donor, update_block, end_block, cid, chain_id, manual_add) DO NOTHING
                ",
                    &[&event.donor, &event.update_block, &event.end_block, &event.cid, &event.chain_id, &event.manual_add]
                )
}

pub fn cid_exists(client: &mut postgres::Client, cid: &str)->Result<bool, postgres::Error>{
    let row = client.query_one("
                    SELECT count(cid) FROM event_update_valid_block WHERE cid=$1::TEXT;
                ",
                    &[&cid]
                )?;
    
    let count: i64 = row.try_get(0)?;
    if count>0{
        return Ok(true);
    }
    Ok(false)
}

pub fn get_max_update_block(client: &mut postgres::Client, table: String, chain_id: i64)-> Result<i64, postgres::Error>{
    let row = client.query_one(&format!("SELECT MAX(update_block) FROM {} WHERE chain_id=$1::BIGINT", table), 
    &[&chain_id])?;
    let maxb: i64 = row.try_get(0)?;
    Ok(maxb)
}

pub fn delete_cid(client: &mut postgres::Client, chain_id: i64, node: String, cid: String, end_block: i64)->Result<u64, postgres::Error>{
    client.execute("DELETE FROM pinned_cids
                            WHERE chain_id=$1::BIGINT AND node=$2::TEXT AND cid=$3::TEXT AND end_block=$4::BIGINT);", 
                    &[&chain_id, &node, &cid, &end_block])
}

pub fn add_cid(client: &mut postgres::Client, chain_id: i64, node: String, cid: String, end_block: i64)->Result<u64, postgres::Error>{
    client.execute("INSERT INTO pinned_cids (chain_id, node, cid, end_block)
                                          VALUES ($1::BIGINT, $2::TEXT, $3::TEXT, $4::BIGINT)", 
                            &[&chain_id, &node, &cid, &end_block])
}

pub fn add_failed_pin(client: &mut postgres::Client, chain_id: i64, node: &str, cid: &str, end_block: i64)->Result<u64, postgres::Error>{
    client.execute("INSERT INTO failed_pins (chain_id, node, cid, end_block)
                                    VALUES ($1::BIGINT, $2::TEXT, $3::TEXT, $4::BIGINT)
                                    ON CONFLICT (chain_id, node, cid, end_block) DO NOTHING", 
                &[&chain_id, &node, &cid, &end_block])
}

pub fn delete_multichain_expired_cids(client: &mut postgres::Client, chain_id: i64, end_block: i64)->Result<u64, postgres::Error>{
    client.execute("DELETE FROM pinned_cids AS p1
                        USING pinned_cids AS p2
                        WHERE p1.chain_id=$1::BIGINT AND p1.end_block<=$2::BIGINT AND p1.chain_id!=p2.chain_id AND p1.cid=p2.cid;",
            &[&chain_id, &end_block])
}

pub fn get_single_chain_expired_cids(client: &mut postgres::Client, chain_id: i64, end_block: i64)->Result<Vec<CIDInfo>, postgres::Error>{
    let res = client.query("SELECT p1.cid, p1.end_block, p1.node From pinned_cids AS p1
                                                    INNER JOIN pinned_cids p2 ON p1.chain_id!=p2.chain_id AND p1.cid!=p2.cid
                                                    WHERE p1.chain_id=$1::BIGINT AND p1.end_block<=$2::BIGINT
                                                    GROUP BY p1.chain_id, p1.node, p1.cid, p1.end_block", 
                                &[&chain_id, &end_block])?;
            
    let mut v = vec![];
    for r in res{
        v.push(CIDInfo{
            chain_id: Option::Some(chain_id.clone()),
            cid: r.get(0),
            end_block: r.get(1),
            node: r.get(2),
            node_login: Option::None,
            node_pass: Option::None
        });
    }
    Ok(v)
}

pub fn update_existing_cids_end_block(client: &mut postgres::Client, chain_id: i64, end_block: i64)->Result<u64, postgres::Error>{
    client.execute("UPDATE pinned_cids as pc
                        SET end_block=euvb.end_block
                        FROM  event_update_valid_block as euvb
                        WHERE euvb.chain_id=pc.chain_id AND pc.cid=euvb.cid AND pc.end_block<euvb.end_block 
                                AND euvb.end_block>$1::BIGINT AND euvb.chain_id=$2::BIGINT;", 
            &[&end_block, &chain_id])
}

pub fn get_new_cids(client: &mut postgres::Client, chain_id: i64, end_block: i64)->Result<Vec<CIDInfo>, postgres::Error>{
    let r = client.query("SELECT euvb.cid, MAX(euvb.end_block)
                                                FROM event_update_valid_block as euvb
                                                LEFT JOIN pinned_cids pc ON euvb.chain_id=pc.chain_id AND euvb.cid=pc.cid
                                                WHERE euvb.end_block>$1::BIGINT AND euvb.chain_id=$2::BIGINT AND pc.cid IS NULL
                                                GROUP BY euvb.cid;", 
                                                        &[&end_block, &chain_id])?;
    let mut rows = vec![];
    for row in r{
        rows.push(CIDInfo{
            chain_id: Option::Some(chain_id.clone()),
            cid: row.get(0),
            end_block: row.get(1),
            node: Option::None,
            node_login: Option::None,
            node_pass: Option::None
        })
    }
    Ok(rows)
}

pub fn delete_expired_failed_cids(client: &mut postgres::Client, chain_id: i64, end_block: i64)->Result<u64, postgres::Error>{
    client.execute("DELETE FROM failed_pins WHERE end_block<=$1::BIGINT AND chain_id=$2::BIGINT", 
            &[&end_block, &chain_id])
}

pub fn get_failed_cids(client: &mut postgres::Client, chain_id: i64, end_block: i64)->Result<Vec<CIDInfo>, postgres::Error>{
    let r = client.query("SELECT node, cid, end_block
                                                FROM failed_pins
                                                WHERE end_block>$1::BIGINT AND chain_id=$2::BIGINT", 
                                                        &[&end_block, &chain_id])?;
    let mut rows = vec![];
    for row in r{
        rows.push(CIDInfo{
            chain_id: Option::Some(chain_id.clone()),
            node: row.get(0),
            cid: row.get(1),
            end_block: row.get(2),
            node_login: Option::None,
            node_pass: Option::None
        })
    }
    Ok(rows)
}
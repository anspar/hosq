const ipfsManager = artifacts.require("IpfsManager");

/*
 * uncomment accounts to access the test accounts made available by the
 * Ethereum client
 * See docs: https://www.trufflesuite.com/docs/truffle/testing/writing-tests-in-javascript
 */
contract("IpfsManager", function (accounts) {
  let BN = web3.utils.BN;
  var contract = null;
  var provider_id = null;
  before("should assert true", async function () {
    contract = await ipfsManager.deployed();
    let log = await contract.add_pinning_service("http://192.168.178.41:5001", 10*1e9);
    for(let l of log.logs){
      if(l.event==="AddProvider"){
        provider_id = l.args.id.toString();
        // console.log(provider_id);
      }
    }
    return assert.isTrue(true);
  });

  it("should get correct block price", async function () {
    let fee = await contract.get_per_block_price(provider_id);
    return assert.equal(fee.toString(), web3.utils.toWei("10", "gwei").toString())
  });

  it("should update block price", async function () {
    await contract.update_block_price(web3.utils.toWei("999", "gwei"), provider_id, {from: accounts[0]});
    let fee = await contract.get_per_block_price(provider_id);
    return assert.equal(fee.toString(), web3.utils.toWei("999", "gwei").toString())
  });

  it("should calculate correct total price", async function () {
    await contract.update_block_price(web3.utils.toWei("10", "gwei"), provider_id, {from: accounts[0]});
    let res = await contract.get_total_price_for_blocks(3, provider_id);
    // console.log(res['0'].toString(), res['1'].toString());
    return assert.equal(res['0'].toString(), web3.utils.toWei("30", "gwei").toString());
  });

  it("should add new valid blocks", async function () {
    await contract.update_block_price(web3.utils.toWei("9", "gwei"), provider_id, {from: accounts[0]});
    let res = await contract.get_total_price_for_blocks("10", provider_id);
    await contract.add_new_valid_block("QmdDURevega5bLgTWkAnBvct2MDp2X2zpWDuYWv5f5oCZj", "10", provider_id,
                                                  {from: accounts[0], 
                                                   value: res['0'].add(res['1']).toString()});
    let valid_b = await contract.get_valid_block("QmdDURevega5bLgTWkAnBvct2MDp2X2zpWDuYWv5f5oCZj", provider_id);
    let log = await contract.add_new_valid_block("QmdDURevega5bLgTWkAnBvct2MDp2X2zpWDuYWv5f5oCZj", "10", provider_id,
                                                  {from: accounts[0], 
                                                   value: res['0'].add(res['1']).toString()});
    
    for(let l of log.logs){
      if (l.event === "UpdateValidBlock"){
        // console.log(l.args.end_block.toString(), valid_b.toString());
        return assert.isTrue(l.args.end_block.eq(valid_b.add(new BN("10"))));
      }
    }
    return false;
  });
});
